//! Native action policy, macOS platform bridge, CoreBluetooth dispatch and the
//! process-owned runtime for the `ble-lock` tool.
//!
//! The OS-independent controller and Keychain item model live in `controller`,
//! the real macOS (or stable-unavailable) capabilities in `platform`, the
//! main-thread CoreBluetooth dispatch and event translation in `central_bridge`,
//! the shared runtime state/status/lifecycle in `runtime` and the IPC surface in
//! `commands`.
//!
//! Controller types are resolved through this module boundary (`super::`),
//! because the native Rust module path is not frozen: the sole registration
//! path is the `#[path]` include in `app_runtime/run_app.rs`
//! (`pub(crate) mod ble_lock_native;`).
//!
//! The behavior tests assert stable action outcomes and public snapshot/flags
//! through the injected OS seam; they never inspect private functions or code
//! shape, and they never assert on constants alone.

#[path = "native/central_bridge.rs"]
mod central_bridge;
#[path = "native/commands.rs"]
mod commands;
#[path = "native/controller.rs"]
mod controller;
#[path = "native/platform.rs"]
mod platform;
#[path = "native/runtime.rs"]
mod runtime;

/// Event carrying the one shared [`BleLockStatus`] snapshot (tool + tray).
pub const BLE_LOCK_STATUS_EVENT: &str = "ble-lock-status-updated";
/// Event carrying the discoverable device snapshots.
pub const BLE_LOCK_DEVICES_EVENT: &str = "ble-lock-devices-updated";

pub use commands::*;
pub use commands::{
    __cmd__ble_lock_clear_device, __cmd__ble_lock_delete_password, __cmd__ble_lock_get_status,
    __cmd__ble_lock_lock_now, __cmd__ble_lock_request_accessibility,
    __cmd__ble_lock_save_config, __cmd__ble_lock_scan_start, __cmd__ble_lock_scan_stop,
    __cmd__ble_lock_select_device, __cmd__ble_lock_set_password,
};
#[allow(unused_imports)]
pub use controller::{
    BleLockKeychainItem, BleLockKeychainProfile, BleLockNativeController, BleLockNativeOutcome,
    BleLockNativeSnapshot, BleLockPlatform,
};
#[allow(unused_imports)]
pub use platform::MacPlatform;
pub use runtime::{install, shutdown_runtime};

#[cfg(test)]
#[path = "tests/native.rs"]
mod tests;
