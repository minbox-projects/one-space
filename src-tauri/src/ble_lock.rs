//! `ble-lock` backend: validated profile-local configuration, a single-device
//! presence state machine, and the macOS-gated CoreBluetooth bridge.
//!
//! Frozen plan `20261010-ble-lock-tool` Step 1. Process-owned startup/lifecycle,
//! native system actions and Keychain live in Step 2; the frontend tool, tray and
//! shared runtime status live in Step 3.

mod central;
mod config;
mod state;

pub use central::{BleBluetoothAuthorization, BleCentral, BleCentralEvent};
pub use config::{
    BleLockConfig, BleLockConfigStore, BleLockError, BleLockMode, BLE_LOCK_CONFIG_SCHEMA_VERSION,
    MAX_LOCK_DELAY_SECS, MAX_NO_SIGNAL_TIMEOUT_SECS, MAX_RSSI_DBM, MIN_LOCK_DELAY_SECS,
    MIN_NO_SIGNAL_TIMEOUT_SECS, MIN_RSSI_DBM, RSSI_STEP_DBM,
};
pub use state::{
    BleBluetoothState, BleDevice, BleLockEffect, BleLockEvent, BleLockMonitor, BleLockStatus,
    BlePresence, INVALID_RSSI_SENTINEL, MAX_SMOOTHING_READINGS,
};
