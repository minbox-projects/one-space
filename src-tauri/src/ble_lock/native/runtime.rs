//! Process-owned runtime: shared state, status publishing and lifecycle.

use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter};

use crate::ble_lock::{
    BleDevice, BleLockConfig, BleLockConfigStore, BleLockEffect, BleLockError, BleLockEvent,
    BleLockMode, BleLockMonitor, BleLockStatus, BlePresence,
};

use super::central_bridge::{dispatch_release_central, dispatch_request_rssi, dispatch_sync_central};
use super::controller::{
    BleLockKeychainProfile, BleLockNativeController, BleLockNativeOutcome, BleLockPlatform,
};
use super::platform::MacPlatform;

/// RSSI/away-lost cadence for the process-owned runtime, independent of UI.
const BLE_LOCK_TICK_INTERVAL: Duration = Duration::from_secs(1);

/// Everything the process-owned runtime shares across threads (all `Send`).
pub(super) struct RuntimeState {
    pub(super) app: AppHandle,
    pub(super) config: BleLockConfig,
    pub(super) monitor: BleLockMonitor,
    pub(super) controller: BleLockNativeController<MacPlatform>,
    pub(super) devices: Vec<BleDevice>,
    /// User-requested discovery scan, distinct from monitored-resource desire.
    pub(super) discovering: bool,
    pub(super) last_presence: BlePresence,
    /// Last published snapshot so unchanged status is not re-emitted each tick.
    pub(super) last_status: Option<BleLockStatus>,
}

/// A monitored resource is desired only when a device is selected *and* at least
/// one background action (lock, unlock or wake) is enabled.
fn desired_monitoring(rt: &RuntimeState) -> bool {
    rt.config.device_id.is_some()
        && (rt.config.lock_rssi.is_some()
            || rt.config.unlock_rssi.is_some()
            || rt.config.wake_on_proximity)
}

/// A scan is wanted for user discovery, or for passive advertisement monitoring.
fn desired_scan(rt: &RuntimeState) -> bool {
    rt.discovering || (desired_monitoring(rt) && rt.config.mode == BleLockMode::Passive)
}

/// Resolve the hardware plan for the current runtime state.
pub(super) fn hardware_plan(
    rt: &RuntimeState,
) -> (Option<(String, Option<String>)>, BleLockMode, bool, bool) {
    (
        configured_selection(rt),
        rt.config.mode,
        desired_scan(rt),
        desired_monitoring(rt),
    )
}

fn selection_device(config: &BleLockConfig, previous_rssi: Option<i16>) -> Option<BleDevice> {
    config.device_id.as_ref().map(|id| BleDevice {
        id: id.clone(),
        name: config.device_name.clone().unwrap_or_else(|| id.clone()),
        rssi: previous_rssi.unwrap_or(0),
    })
}

static RUNTIME: OnceLock<Mutex<RuntimeState>> = OnceLock::new();

pub(super) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

pub(super) fn runtime_guard() -> Option<MutexGuard<'static, RuntimeState>> {
    RUNTIME.get().and_then(|runtime| runtime.lock().ok())
}

/// Install the process-owned runtime exactly once, after Tauri setup.
///
/// Safe to call repeatedly; the second and later calls are no-ops.
pub fn install(app: AppHandle) {
    if RUNTIME.get().is_some() {
        return;
    }
    let profile = if crate::config::is_dev_build() {
        BleLockKeychainProfile::Debug
    } else {
        BleLockKeychainProfile::Release
    };
    let config = match BleLockConfigStore::for_current_profile().and_then(|store| store.load()) {
        Ok(config) => config,
        Err(error) => {
            log::warn!("ble-lock: falling back to default config: {error}");
            BleLockConfig::default()
        }
    };
    let controller = BleLockNativeController::new(config.clone(), profile, MacPlatform);
    log::info!(
        "ble-lock runtime installed (Keychain profile {:?})",
        controller.profile()
    );
    let snapshot = controller.snapshot();
    let mut monitor = BleLockMonitor::new(config.clone());
    if let Some(device) = selection_device(&config, None) {
        monitor.select_device(device);
    }
    monitor.set_screen_locked(snapshot.screen_locked);
    monitor.set_accessibility_trusted(snapshot.accessibility_trusted);
    monitor.set_password_configured(snapshot.password_configured);
    let last_presence = monitor.status().presence;
    let state = RuntimeState {
        app: app.clone(),
        config,
        monitor,
        controller,
        devices: Vec::new(),
        discovering: false,
        last_presence,
        last_status: None,
    };
    if RUNTIME.set(Mutex::new(state)).is_err() {
        return;
    }
    install_system_observers(app.clone());
    start_tick(app.clone());
    // Schedule the saved enabled action(s)/wake and any passive monitoring so a
    // restart restores the persisted selection and its hardware resources.
    let plan = runtime_guard().map(|guard| hardware_plan(&guard));
    if let Some((selection, mode, scan, background)) = plan {
        if background || scan {
            dispatch_sync_central(&app, selection, mode, scan, background, true);
        }
    }
    publish_status(&app);
}

/// Stop the runtime and release the bridge. Idempotent.
pub fn shutdown_runtime() {
    let app = {
        let mut guard = match runtime_guard() {
            Some(guard) => guard,
            None => return,
        };
        guard.controller.shutdown();
        guard.discovering = false;
        guard.monitor.set_scanning(false);
        guard.last_status = None;
        guard.app.clone()
    };
    dispatch_release_central(&app);
    publish_status(&app);
}

/// Suppress proximity decisions and release the bridge on observed system sleep.
fn on_system_sleep() {
    let app = {
        let mut guard = match runtime_guard() {
            Some(guard) => guard,
            None => return,
        };
        guard.controller.note_system_sleep();
        guard.monitor.set_scanning(false);
        guard.last_status = None;
        guard.app.clone()
    };
    // Release the bridge entirely; system wake rebuilds it from scratch.
    dispatch_release_central(&app);
    publish_status(&app);
}

/// Rebuild the monitor/scan/connection and reset stale eligibility after wake.
fn on_system_wake() {
    let plan = {
        let mut guard = match runtime_guard() {
            Some(guard) => guard,
            None => return,
        };
        // A stopped controller must never rebuild monitor/device state, so a
        // queued wake after exit cannot resurrect the runtime.
        if !guard.controller.note_system_wake() {
            return;
        }
        // Drop the old manager, then rebuild monitor history and selection so no
        // pre-sleep reading, timer, arming or callback can survive.
        let previous_rssi = guard
            .monitor
            .status()
            .selected_device
            .as_ref()
            .map(|device| device.rssi);
        let mut monitor = BleLockMonitor::new(guard.config.clone());
        if let Some(device) = selection_device(&guard.config, previous_rssi) {
            monitor.select_device(device);
        }
        guard.monitor = monitor;
        guard.last_presence = guard.monitor.status().presence;
        guard.devices.clear();
        guard.last_status = None;
        let app = guard.app.clone();
        let plan = hardware_plan(&guard);
        Some((app, plan))
    };
    let Some((app, (selection, mode, scan, background))) = plan else {
        return;
    };
    // `rebuild = true` takes and drops the old bridge before creating a fresh one.
    dispatch_sync_central(&app, selection, mode, scan, background, true);
    emit_devices(&app);
    publish_status(&app);
}

fn configured_selection(rt: &RuntimeState) -> Option<(String, Option<String>)> {
    rt.config
        .device_id
        .as_ref()
        .map(|id| (id.clone(), rt.config.device_name.clone()))
}

fn build_status(rt: &RuntimeState) -> BleLockStatus {
    let snapshot = rt.controller.snapshot();
    let mut status = rt.monitor.status();
    status.screen_locked = snapshot.screen_locked;
    status.password_configured = snapshot.password_configured;
    status.accessibility_trusted = snapshot.accessibility_trusted;
    status.monitoring = desired_monitoring(rt);
    status.scanning = desired_scan(rt);
    status.config = rt.controller.config().clone();
    if status.last_error.is_none() {
        status.last_error = snapshot.last_error;
    }
    status
}

pub(super) fn current_status() -> Option<BleLockStatus> {
    runtime_guard().map(|guard| build_status(&guard))
}

pub(super) fn fallback_status() -> BleLockStatus {
    BleLockMonitor::new(BleLockConfig::default()).status()
}

/// Publish the shared snapshot, but only when it actually changed (so async or
/// manual unlock and Accessibility changes are surfaced without per-tick spam).
pub(super) fn publish_status(app: &AppHandle) {
    let status = {
        let Some(mut guard) = runtime_guard() else {
            return;
        };
        let status = build_status(&guard);
        if guard.last_status.as_ref() == Some(&status) {
            return;
        }
        guard.last_status = Some(status.clone());
        status
    };
    let _ = app.emit(super::BLE_LOCK_STATUS_EVENT, status);
}

/// Set a runtime error and force the next status publish to carry it.
pub(super) fn set_runtime_error(error: BleLockError) {
    if let Some(mut guard) = runtime_guard() {
        guard.monitor.set_last_error(error);
        guard.last_status = None;
    }
}

pub(super) fn emit_devices(app: &AppHandle) {
    if let Some(guard) = runtime_guard() {
        let _ = app.emit(super::BLE_LOCK_DEVICES_EVENT, guard.devices.clone());
    }
}

/// Feed the observed presence plus smoothed mean to the tested controller on
/// every reading/tick. The controller decides the unlock-threshold crossing
/// itself, so a `Near` reading between lock and unlock never injects.
fn sync_presence(rt: &mut RuntimeState) -> BleLockNativeOutcome {
    let status = rt.monitor.status();
    rt.last_presence = status.presence;
    rt.controller
        .observe_presence(status.presence, status.averaged_rssi)
}

pub(super) fn apply_reading(rt: &mut RuntimeState, event: BleLockEvent, now_ms: u64) {
    if rt.controller.is_system_sleeping() {
        return;
    }
    let effects = rt.monitor.feed(event, now_ms);
    sync_presence(rt);
    for effect in effects {
        if effect == BleLockEffect::Lock {
            rt.controller.lock_for_presence();
        }
    }
}

fn start_tick(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(BLE_LOCK_TICK_INTERVAL);
        if !tick_once(&app) {
            break;
        }
    });
}

/// Advance the state machine once. Returns `false` after shutdown.
fn tick_once(app: &AppHandle) -> bool {
    let now = now_ms();
    let mut request_active_rssi = false;
    let mut retry_connect = false;
    let mut running = true;
    if let Some(mut guard) = runtime_guard() {
        running = !guard.controller.snapshot().stopped;
        if !running {
            return false;
        }
        if guard.controller.is_system_sleeping() {
            // System sleep skips monitor.tick and every OS action, including an
            // automatic lock; the status carries the stable `system_sleep` code.
            drop(guard);
            publish_status(app);
            return true;
        }
        let effects = guard.monitor.tick(now);
        for effect in effects {
            match effect {
                BleLockEffect::Lock => {
                    guard.controller.lock_for_presence();
                }
                // A throttled automatic reconnect for a selected active device
                // whose transport reported it unavailable; dispatched below,
                // outside this runtime lock.
                BleLockEffect::Connect => retry_connect = true,
                BleLockEffect::Unlock => {}
            }
        }
        // Tick the adapter even without a Lock effect so a threshold crossing
        // while presence stays Near is honoured (the display wakes only on edge).
        sync_presence(&mut guard);
        // Poll the observed native state for an async/manual unlock and for a
        // display-off whose ordinary lock was only observed on a later tick.
        let _ = guard.controller.poll_native_state();
        let screen_locked = guard.controller.snapshot().screen_locked;
        guard.monitor.set_screen_locked(screen_locked);
        // No forced publish here: `publish_status` below emits only when the
        // full snapshot actually changed, so a steady tick/RSSI keeps quiet.
        if desired_monitoring(&guard) && guard.config.mode == BleLockMode::Active {
            request_active_rssi = true;
        }
    }
    // The emitted retry reuses the background-aware hardware plan; same-id
    // selection keeps an in-flight CoreBluetooth request intact.
    if retry_connect {
        let plan = runtime_guard().map(|guard| hardware_plan(&guard));
        if let Some((selection, mode, scan, background)) = plan {
            dispatch_sync_central(app, selection, mode, scan, background, false);
        }
    }
    if request_active_rssi {
        dispatch_request_rssi(app);
    }
    publish_status(app);
    running
}

#[cfg(target_os = "macos")]
fn install_system_observers(_app: AppHandle) {
    use block2::RcBlock;
    use objc2_app_kit::{
        NSWorkspace, NSWorkspaceDidWakeNotification, NSWorkspaceWillSleepNotification,
    };
    use objc2_foundation::NSNotification;
    use std::ptr::NonNull;

    let workspace = NSWorkspace::sharedWorkspace();
    let center = workspace.notificationCenter();
    let sleep_block = RcBlock::new(move |_notification: NonNull<NSNotification>| on_system_sleep());
    let sleep_observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceWillSleepNotification),
            None,
            None,
            &sleep_block,
        )
    };
    let wake_block = RcBlock::new(move |_notification: NonNull<NSNotification>| on_system_wake());
    let wake_observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidWakeNotification),
            None,
            None,
            &wake_block,
        )
    };
    std::mem::forget(sleep_observer);
    std::mem::forget(wake_observer);
    std::mem::forget(sleep_block);
    std::mem::forget(wake_block);
}

#[cfg(not(target_os = "macos"))]
fn install_system_observers(_app: AppHandle) {}

pub(super) fn mark_screen_locked(rt: &mut RuntimeState) {
    let locked = rt.controller.snapshot().screen_locked;
    rt.monitor.set_screen_locked(locked);
}

/// Validate and persist a configuration *before* any runtime mutation, so a
/// rejected save leaves the last valid config and running resources untouched;
/// then apply it to the runtime and schedule the resulting hardware plan.
///
/// The controller keeps its manual-lock guard when the selected device id is
/// unchanged; a different id resets eligibility inside `set_config`.
pub(super) fn commit_config(config: BleLockConfig) -> Result<AppHandle, BleLockError> {
    let (app, selection, mode, scan, background, rebuild) = {
        let mut guard = runtime_guard().ok_or_else(BleLockError::platform_unavailable)?;
        config.validate()?;
        BleLockConfigStore::for_current_profile()?.save(&config)?;
        let previous_device = guard.config.device_id.clone();
        let previous_mode = guard.config.mode;
        let previous_rssi = guard
            .monitor
            .status()
            .selected_device
            .as_ref()
            .map(|device| device.rssi);
        guard.config = config.clone();
        guard.controller.set_config(config.clone());
        let mut monitor = BleLockMonitor::new(config.clone());
        if let Some(device) = selection_device(&config, previous_rssi) {
            monitor.select_device(device);
        }
        monitor.set_scanning(desired_scan(&guard));
        let screen_locked = guard.controller.snapshot().screen_locked;
        monitor.set_screen_locked(screen_locked);
        monitor.set_accessibility_trusted(guard.controller.platform().accessibility_trusted());
        monitor.set_password_configured(guard.controller.password_configured());
        guard.monitor = monitor;
        guard.last_presence = guard.monitor.status().presence;
        if previous_device != config.device_id {
            guard.devices.clear();
        }
        guard.last_status = None;
        let rebuild = previous_device != config.device_id || previous_mode != config.mode;
        let plan = hardware_plan(&guard);
        (guard.app.clone(), plan.0, plan.1, plan.2, plan.3, rebuild)
    };
    dispatch_sync_central(&app, selection, mode, scan, background, rebuild);
    Ok(app)
}
