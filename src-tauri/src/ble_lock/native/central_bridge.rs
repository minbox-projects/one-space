//! Main-thread CoreBluetooth bridge dispatch and event translation.

use std::sync::atomic::{AtomicU64, Ordering};

use tauri::AppHandle;

use crate::ble_lock::{
    BleBluetoothAuthorization, BleBluetoothState, BleCentral, BleCentralEvent, BleLockError,
    BleLockEvent, BleLockMode,
};

use super::runtime::{
    apply_reading, emit_devices, hardware_plan, now_ms, publish_status, runtime_guard,
    set_runtime_error,
};

thread_local! {
    /// Main-thread-confined CoreBluetooth bridge. Never moved off the main queue,
    /// so no unsafe `Send` is manufactured for the `!Send` CoreBluetooth handles.
    static MAIN_CENTRAL: std::cell::RefCell<Option<BleCentral>> = const { std::cell::RefCell::new(None) };
}

/// Generation token bumped on every bridge rebuild. A handler created for a
/// replaced manager captured an older generation and ignores its delayed
/// callbacks, so a stale callback can never act on the new selection even when
/// the CoreBluetooth UUID is identical.
static CENTRAL_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Translate a bridge event into monitor/controller/status updates.
fn handle_central_event(app: &AppHandle, event: BleCentralEvent) {
    let now = now_ms();
    let mut devices_changed = false;
    let mut reapply = false;
    if let Some(mut guard) = runtime_guard() {
        // Ignore any bridge event while asleep or stopped: the resource is
        // released and a rebuild clears stale eligibility on wake.
        if guard.controller.snapshot().stopped || guard.controller.is_system_sleeping() {
            return;
        }
        match event {
            BleCentralEvent::State(state) => {
                guard.monitor.set_bluetooth_state(state);
                // One radio-feedback adapter owns the stable error taxonomy
                // (including the `bluetooth_unsupported` code the old inline
                // match missed) and clears only stale radio errors on recovery.
                guard.controller.observe_bluetooth_state(state);
                match state {
                    BleBluetoothState::PoweredOff => guard.monitor.set_scanning(false),
                    BleBluetoothState::PoweredOn => {
                        // Re-apply the desired hardware plan now that the radio
                        // is ready; the adapter already cleared a stale error.
                        reapply = true;
                    }
                    _ => {}
                }
                guard.last_status = None;
            }
            BleCentralEvent::Authorization(authorization) => {
                if matches!(
                    authorization,
                    BleBluetoothAuthorization::Denied | BleBluetoothAuthorization::Restricted
                ) {
                    // Route the denied/restricted case through the same adapter
                    // so a later PoweredOn recovery clears it consistently.
                    guard
                        .controller
                        .observe_bluetooth_state(BleBluetoothState::Unauthorized);
                }
                guard.last_status = None;
            }
            BleCentralEvent::Discovered(device) => {
                match guard.devices.iter_mut().find(|entry| entry.id == device.id) {
                    Some(existing) => *existing = device,
                    None => guard.devices.push(device),
                }
                devices_changed = true;
            }
            BleCentralEvent::Advertisement { device_id, rssi } => {
                apply_reading(
                    &mut guard,
                    BleLockEvent::Advertisement { device_id, rssi },
                    now,
                );
            }
            BleCentralEvent::ConnectionRead { device_id, rssi } => {
                apply_reading(
                    &mut guard,
                    BleLockEvent::ConnectionRead { device_id, rssi },
                    now,
                );
            }
            BleCentralEvent::Connected { .. } => {
                guard.monitor.clear_last_error();
                guard.last_status = None;
            }
            BleCentralEvent::Disconnected { .. } => {
                // The monitor's periodic tick owns the throttled Connect retry:
                // a quick failure inside the throttle window consumes the last
                // CoreBluetooth callback with no future callback until the next
                // attempt, so this callback only records the transient error.
                guard
                    .monitor
                    .set_last_error(BleLockError::device_unavailable());
                guard.last_status = None;
            }
            BleCentralEvent::Error(error) => {
                guard.monitor.set_last_error(error);
                guard.last_status = None;
            }
        }
    }
    if devices_changed {
        emit_devices(app);
    }
    if reapply {
        let plan = runtime_guard().map(|guard| hardware_plan(&guard));
        if let Some((selection, mode, scan, background)) = plan {
            dispatch_sync_central(app, selection, mode, scan, background, false);
        }
    }
    publish_status(app);
}

pub(super) fn dispatch_request_rssi(app: &AppHandle) {
    let _ = app.run_on_main_thread(|| {
        MAIN_CENTRAL.with(|cell| {
            if let Some(central) = cell.borrow().as_ref() {
                central.request_rssi();
            }
        });
    });
}

/// Release the main-thread bridge (stop + cancel + drop) without creating one.
pub(super) fn dispatch_release_central(app: &AppHandle) {
    let _ = app.run_on_main_thread(|| {
        MAIN_CENTRAL.with(|cell| {
            if let Some(central) = cell.borrow_mut().take() {
                central.stop_scan();
                central.clear_device();
                drop(central);
            }
        });
    });
}

/// Ensure the bridge exists on the main thread, then apply selection/mode/scan.
///
/// When `rebuild` is set the previous manager is stopped, cancelled and dropped
/// before a fresh one is created, so its delayed callbacks can never act on the
/// new state. A new manager always gets a fresh generation token.
pub(super) fn dispatch_sync_central(
    app: &AppHandle,
    selection: Option<(String, Option<String>)>,
    mode: BleLockMode,
    scan: bool,
    background: bool,
    rebuild: bool,
) {
    let handler_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        // After shutdown the bridge must stay released: a queued dispatch can
        // never allocate and resurrect a central. The runtime mutex is taken
        // only for this boolean check and is never held across the main-thread
        // dispatch that follows.
        let stopped = runtime_guard()
            .map(|guard| guard.controller.snapshot().stopped)
            .unwrap_or(true);
        if stopped {
            return;
        }
        MAIN_CENTRAL.with(|cell| {
            if rebuild {
                if let Some(central) = cell.borrow_mut().take() {
                    central.stop_scan();
                    central.clear_device();
                    drop(central);
                }
            }
            let needs_create = cell.borrow().is_none();
            if needs_create {
                let generation = CENTRAL_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
                let app_for_handler = handler_app.clone();
                let central = BleCentral::new(move |event| {
                    if CENTRAL_GENERATION.load(Ordering::SeqCst) != generation {
                        return;
                    }
                    handle_central_event(&app_for_handler, event);
                });
                *cell.borrow_mut() = Some(central);
            }
            if let Some(central) = cell.borrow().as_ref() {
                // Keep the runtime monitor's radio state in sync with the real
                // manager before any connection attempt. A same-bridge monitor
                // rebuild (same device and mode) starts `Unknown`, and no state
                // event follows for a retained manager, which would otherwise
                // disable the powered-on retry guard forever.
                let state = central.state();
                if let Some(mut guard) = runtime_guard() {
                    guard.monitor.set_bluetooth_state(state);
                }
                central.set_mode(mode);
                if state == BleBluetoothState::PoweredOn {
                    match &selection {
                        Some((id, name)) if background => {
                            if let Err(error) = central.select_device(id, name.as_deref()) {
                                set_runtime_error(error);
                            }
                        }
                        _ => central.clear_device(),
                    }
                    if scan {
                        central.start_scan();
                    } else {
                        central.stop_scan();
                    }
                } else {
                    // Before PoweredOn never blind start/connect; the state event
                    // re-applies the plan once the radio is ready.
                    central.stop_scan();
                }
            }
        });
    });
}
