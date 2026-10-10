//! Validated profile-local configuration for the `ble-lock` tool (Step 1).
//!
//! The configuration is a standalone, unsynchronized `ble_lock.json` that lives
//! in the current build profile's application directory (see
//! [`BleLockConfigStore::for_current_profile`]). It never contains a password:
//! the macOS login password is owned by the Keychain seam added in Step 2.
//!
//! `save` validates before touching the filesystem, so a rejected configuration
//! can never replace the last valid bytes on disk.

use std::fmt;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Current on-disk schema version for `ble_lock.json`.
pub const BLE_LOCK_CONFIG_SCHEMA_VERSION: u32 = 1;

/// Lowest accepted RSSI threshold, in dBm.
pub const MIN_RSSI_DBM: i16 = -95;
/// Highest accepted RSSI threshold, in dBm.
pub const MAX_RSSI_DBM: i16 = -30;
/// Step between accepted RSSI thresholds, in dBm.
pub const RSSI_STEP_DBM: i16 = 5;

/// Inclusive lower bound for the lock delay, in seconds.
pub const MIN_LOCK_DELAY_SECS: u32 = 1;
/// Inclusive upper bound for the lock delay, in seconds.
pub const MAX_LOCK_DELAY_SECS: u32 = 3600;
/// Inclusive lower bound for the no-signal timeout, in seconds.
pub const MIN_NO_SIGNAL_TIMEOUT_SECS: u32 = 5;
/// Inclusive upper bound for the no-signal timeout, in seconds.
pub const MAX_NO_SIGNAL_TIMEOUT_SECS: u32 = 3600;

/// Monitoring mode for the single selected device.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BleLockMode {
    /// Connect to the peripheral and derive RSSI from connection reads.
    #[default]
    Active,
    /// Derive RSSI from advertisements only and never connect.
    Passive,
}

/// A stable, non-secret error carrying an error code and an optional safe detail.
///
/// The code values come from the frozen public contract; `detail` never carries
/// a password or other secret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BleLockError {
    /// Stable machine-readable error code.
    pub code: String,
    /// Optional safe human-readable detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl BleLockError {
    /// Build an error from a stable code and optional safe detail.
    pub fn new(code: impl Into<String>, detail: Option<String>) -> Self {
        Self {
            code: code.into(),
            detail,
        }
    }

    /// Configuration is invalid; no state was changed.
    pub fn invalid_config(detail: impl Into<String>) -> Self {
        Self::new("invalid_config", Some(detail.into()))
    }

    /// The current build target does not provide Bluetooth lock support.
    pub fn platform_unavailable() -> Self {
        Self::new("platform_unavailable", None)
    }

    /// Bluetooth is powered off.
    pub fn bluetooth_powered_off() -> Self {
        Self::new("bluetooth_powered_off", None)
    }

    /// Bluetooth permission was denied for this app.
    pub fn bluetooth_unauthorized() -> Self {
        Self::new("bluetooth_unauthorized", None)
    }

    /// No device has been selected.
    pub fn device_not_selected() -> Self {
        Self::new("device_not_selected", None)
    }

    /// The selected device is not currently reachable.
    pub fn device_unavailable() -> Self {
        Self::new("device_unavailable", None)
    }
}

impl fmt::Display for BleLockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.detail {
            Some(detail) => write!(f, "{}: {detail}", self.code),
            None => write!(f, "{}", self.code),
        }
    }
}

impl std::error::Error for BleLockError {}

/// Persisted configuration for the `ble-lock` tool.
///
/// All optional behavior toggles default to `false` and both RSSI actions start
/// disabled (`None`), so a fresh configuration performs no automatic action.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BleLockConfig {
    /// On-disk schema version.
    pub schema_version: u32,
    /// CoreBluetooth UUID of the single monitored device.
    pub device_id: Option<String>,
    /// Cached display name for the selected device.
    pub device_name: Option<String>,
    /// Active or passive monitoring mode.
    pub mode: BleLockMode,
    /// Unlock threshold in dBm; `None` disables automatic unlock.
    pub unlock_rssi: Option<i16>,
    /// Lock threshold in dBm; `None` disables automatic lock.
    pub lock_rssi: Option<i16>,
    /// Seconds below the lock threshold before the away transition.
    pub lock_delay_secs: u32,
    /// Seconds without a valid reading before the lost transition.
    pub no_signal_timeout_secs: u32,
    /// Wake the display when the device becomes near.
    pub wake_on_proximity: bool,
    /// Wake the display but never inject the Keychain password.
    pub wake_without_unlocking: bool,
    /// Pause playing media for the current lock cycle.
    pub pause_now_playing: bool,
    /// Lock through the screensaver instead of an ordinary lock.
    pub use_screensaver_to_lock: bool,
    /// Turn the display off after an ordinary lock.
    pub sleep_display: bool,
}

impl Default for BleLockConfig {
    fn default() -> Self {
        Self {
            schema_version: BLE_LOCK_CONFIG_SCHEMA_VERSION,
            device_id: None,
            device_name: None,
            mode: BleLockMode::Active,
            unlock_rssi: None,
            lock_rssi: None,
            lock_delay_secs: 10,
            no_signal_timeout_secs: 30,
            wake_on_proximity: false,
            wake_without_unlocking: false,
            pause_now_playing: false,
            use_screensaver_to_lock: false,
            sleep_display: false,
        }
    }
}

impl BleLockConfig {
    /// Validate every field against the frozen boundary rules.
    ///
    /// A single threshold never triggers the ordering rule; when both are set,
    /// the unlock threshold must be greater than or equal to the lock threshold.
    pub fn validate(&self) -> Result<(), BleLockError> {
        if let Some(unlock) = self.unlock_rssi {
            validate_threshold("unlock_rssi", unlock)?;
        }
        if let Some(lock) = self.lock_rssi {
            validate_threshold("lock_rssi", lock)?;
        }
        if !(MIN_LOCK_DELAY_SECS..=MAX_LOCK_DELAY_SECS).contains(&self.lock_delay_secs) {
            return Err(BleLockError::invalid_config(format!(
                "lock_delay_secs must be {MIN_LOCK_DELAY_SECS}..={MAX_LOCK_DELAY_SECS}, got {}",
                self.lock_delay_secs
            )));
        }
        if !(MIN_NO_SIGNAL_TIMEOUT_SECS..=MAX_NO_SIGNAL_TIMEOUT_SECS)
            .contains(&self.no_signal_timeout_secs)
        {
            return Err(BleLockError::invalid_config(format!(
                "no_signal_timeout_secs must be {MIN_NO_SIGNAL_TIMEOUT_SECS}..={MAX_NO_SIGNAL_TIMEOUT_SECS}, got {}",
                self.no_signal_timeout_secs
            )));
        }
        if let (Some(unlock), Some(lock)) = (self.unlock_rssi, self.lock_rssi) {
            if unlock < lock {
                return Err(BleLockError::invalid_config(format!(
                    "unlock_rssi ({unlock}) must be >= lock_rssi ({lock}) when both are enabled"
                )));
            }
        }
        Ok(())
    }
}

fn validate_threshold(field: &str, value: i16) -> Result<(), BleLockError> {
    if !(MIN_RSSI_DBM..=MAX_RSSI_DBM).contains(&value) {
        return Err(BleLockError::invalid_config(format!(
            "{field} must be {MIN_RSSI_DBM}..={MAX_RSSI_DBM} dBm, got {value}"
        )));
    }
    if value.rem_euclid(RSSI_STEP_DBM) != 0 {
        return Err(BleLockError::invalid_config(format!(
            "{field} must be a {RSSI_STEP_DBM} dBm step, got {value}"
        )));
    }
    Ok(())
}

/// Profile-local, validated store for `ble_lock.json`.
#[derive(Clone, Debug)]
pub struct BleLockConfigStore {
    root: PathBuf,
}

impl BleLockConfigStore {
    /// Build a store rooted at an explicit directory (used directly by tests and
    /// by Step 2 when it already knows the profile directory).
    pub fn at(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Build a store bound to the current build profile's application directory.
    pub fn for_current_profile() -> Result<Self, BleLockError> {
        let root = crate::config::get_app_dir().map_err(BleLockError::invalid_config)?;
        Ok(Self::at(root))
    }

    /// Absolute path of the configuration file.
    pub fn path(&self) -> PathBuf {
        self.root.join("ble_lock.json")
    }

    /// Load and validate the configuration.
    ///
    /// A missing file yields [`BleLockConfig::default`] and never creates the
    /// file. An existing but unreadable, unparseable or invalid file is an error
    /// so a caller never silently replaces its last valid in-memory state.
    pub fn load(&self) -> Result<BleLockConfig, BleLockError> {
        let path = self.path();
        match fs::read_to_string(&path) {
            Ok(raw) => {
                let parsed: BleLockConfig = serde_json::from_str(&raw)
                    .map_err(|e| BleLockError::invalid_config(format!("ble_lock.json: {e}")))?;
                parsed.validate()?;
                Ok(parsed)
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(BleLockConfig::default()),
            Err(e) => Err(BleLockError::invalid_config(format!("ble_lock.json: {e}"))),
        }
    }

    /// Validate and atomically persist the configuration.
    ///
    /// Validation runs before any filesystem mutation, so an invalid
    /// configuration never rewrites the previous `ble_lock.json` bytes.
    pub fn save(&self, config: &BleLockConfig) -> Result<(), BleLockError> {
        config.validate()?;
        let json = serde_json::to_string_pretty(config)
            .map_err(|e| BleLockError::invalid_config(format!("serialize ble_lock.json: {e}")))?;
        atomic_write(&self.path(), &json)
    }
}

fn atomic_write(path: &Path, content: &str) -> Result<(), BleLockError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| BleLockError::invalid_config(e.to_string()))?;
    }
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, content).map_err(|e| BleLockError::invalid_config(e.to_string()))?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(BleLockError::invalid_config(e.to_string()))
        }
    }
}
