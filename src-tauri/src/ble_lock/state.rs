//! Presence state machine and status snapshot for the `ble-lock` tool (Step 1).
//!
//! [`BleLockMonitor`] is pure Rust: it consumes explicit `now_ms` values and
//! [`BleLockEvent`]s and returns [`BleLockEffect`] intents. It never touches
//! hardware, so the platform bridge in [`super::central`] and the process-owned
//! runtime added in Step 2 stay the only places that talk to CoreBluetooth.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use super::config::{BleLockConfig, BleLockError, BleLockMode};

/// Maximum number of recent readings used by the smoothing average.
pub const MAX_SMOOTHING_READINGS: usize = 5;

/// CoreBluetooth's reserved "RSSI not available" value.
pub const INVALID_RSSI_SENTINEL: i16 = 127;

/// Minimum spacing between automatic reconnect attempts while the selected active
/// device is reported unavailable. A CoreBluetooth pending connect request has no
/// timeout, so early failures must keep retrying without busy-looping.
const RECONNECT_RETRY_INTERVAL_MS: u64 = 5_000;

/// Presence classification for the selected device.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlePresence {
    /// No valid reading has established presence yet.
    #[default]
    Unknown,
    /// The device is present at or above the lock threshold.
    Near,
    /// The device stayed below the lock threshold for the lock delay.
    Away,
    /// No valid reading arrived within the no-signal timeout.
    Lost,
}

/// Bluetooth radio state reported by the platform bridge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BleBluetoothState {
    /// State has not been observed yet.
    #[default]
    Unknown,
    /// The manager is resetting.
    Resetting,
    /// The platform does not support Bluetooth LE central mode.
    Unsupported,
    /// Bluetooth permission has not been granted.
    Unauthorized,
    /// Bluetooth is powered off.
    PoweredOff,
    /// Bluetooth is powered on and ready.
    PoweredOn,
}

/// A nearby BLE device as displayed and selected by the user.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BleDevice {
    /// CoreBluetooth UUID (`CBPeer.identifier`).
    pub id: String,
    /// Display name, falling back to the UUID when unnamed.
    pub name: String,
    /// Latest observed RSSI in dBm.
    pub rssi: i16,
}

/// A single valid RSSI reading delivered by the platform bridge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BleLockEvent {
    /// An advertisement reading; valid only in passive mode.
    Advertisement {
        /// CoreBluetooth UUID of the advertising device.
        device_id: String,
        /// Raw RSSI in dBm.
        rssi: i16,
    },
    /// A connected `readRSSI` result; valid only in active mode.
    ConnectionRead {
        /// CoreBluetooth UUID of the connected device.
        device_id: String,
        /// Raw RSSI in dBm.
        rssi: i16,
    },
}

/// An action the runtime should perform because of a presence transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BleLockEffect {
    /// Establish a connection to the selected peripheral (active mode).
    Connect,
    /// Lock the session.
    Lock,
    /// Perform one guarded unlock attempt.
    Unlock,
}

/// Full status snapshot shared by the tool UI, tray and status events.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BleLockStatus {
    /// Whether the compiled target provides Bluetooth lock support.
    pub platform_available: bool,
    /// Current Bluetooth radio state.
    pub bluetooth_state: BleBluetoothState,
    /// Whether a device is selected and monitoring.
    pub monitoring: bool,
    /// Whether a scan is running.
    pub scanning: bool,
    /// The single selected device, if any.
    pub selected_device: Option<BleDevice>,
    /// Current presence classification.
    pub presence: BlePresence,
    /// Smoothed RSSI (mean of at most five recent valid readings).
    pub averaged_rssi: Option<i16>,
    /// Whether the session is observed to be locked.
    pub screen_locked: bool,
    /// Whether a Keychain password is configured (never the password itself).
    pub password_configured: bool,
    /// Whether Accessibility trust is granted.
    pub accessibility_trusted: bool,
    /// The active configuration.
    pub config: BleLockConfig,
    /// Last safe error, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<BleLockError>,
}

/// Single-device presence state machine.
#[derive(Clone, Debug)]
pub struct BleLockMonitor {
    config: BleLockConfig,
    selected: Option<BleDevice>,
    readings: VecDeque<i16>,
    presence: BlePresence,
    bluetooth_state: BleBluetoothState,
    scanning: bool,
    screen_locked: bool,
    password_configured: bool,
    accessibility_trusted: bool,
    last_error: Option<BleLockError>,
    /// Timestamp when the smoothed RSSI first fell below the lock threshold.
    below_since_ms: Option<u64>,
    /// Timestamp of the most recent valid reading.
    last_reading_ms: Option<u64>,
    /// Whether a lock was already emitted in the current away/lost episode.
    lock_emitted: bool,
    /// Whether the lost transition was already reported in this episode.
    lost_emitted: bool,
    /// Unlock is armed only after an away/lost observation is cleared by near.
    unlock_armed: bool,
    /// Timestamp of the most recent automatic Connect retry, for throttling.
    last_connect_retry_ms: Option<u64>,
}

impl BleLockMonitor {
    /// Create a monitor from a validated configuration.
    pub fn new(config: BleLockConfig) -> Self {
        Self {
            config,
            selected: None,
            readings: VecDeque::with_capacity(MAX_SMOOTHING_READINGS),
            presence: BlePresence::Unknown,
            bluetooth_state: BleBluetoothState::Unknown,
            scanning: false,
            screen_locked: false,
            password_configured: false,
            accessibility_trusted: false,
            last_error: None,
            below_since_ms: None,
            last_reading_ms: None,
            lock_emitted: false,
            lost_emitted: false,
            unlock_armed: false,
            last_connect_retry_ms: None,
        }
    }

    /// Select the single monitored device, clearing all prior history and timers.
    ///
    /// In active mode this emits [`BleLockEffect::Connect`]; passive mode emits
    /// nothing because it never connects. An empty or blank id clears the
    /// selection instead (an unknown device can never trigger an action).
    pub fn select_device(&mut self, device: BleDevice) -> Vec<BleLockEffect> {
        if device.id.trim().is_empty() {
            self.clear_device();
            return Vec::new();
        }
        let connect = self.config.mode == BleLockMode::Active;
        self.selected = Some(device);
        self.reset_history();
        if connect {
            vec![BleLockEffect::Connect]
        } else {
            Vec::new()
        }
    }

    /// Clear the selected device and all derived presence state.
    pub fn clear_device(&mut self) {
        self.selected = None;
        self.reset_history();
    }

    /// Feed one platform reading and return any resulting intents.
    pub fn feed(&mut self, event: BleLockEvent, now_ms: u64) -> Vec<BleLockEffect> {
        let (device_id, rssi) = match &event {
            BleLockEvent::Advertisement { device_id, rssi }
            | BleLockEvent::ConnectionRead { device_id, rssi } => (device_id.clone(), *rssi),
        };

        // The CoreBluetooth "not available" sentinel is not a measurement.
        if rssi == INVALID_RSSI_SENTINEL {
            return Vec::new();
        }

        let selected_id = match &self.selected {
            Some(device) => device.id.clone(),
            None => return Vec::new(),
        };
        if selected_id.is_empty() || device_id != selected_id {
            // No selection, or a stale callback for a replaced/cleared device.
            return Vec::new();
        }

        // Each mode derives RSSI from exactly one source: active mode from
        // connection reads, passive mode from advertisements (REQ-003/AC-003).
        let source_matches_mode = match self.config.mode {
            BleLockMode::Active => matches!(event, BleLockEvent::ConnectionRead { .. }),
            BleLockMode::Passive => matches!(event, BleLockEvent::Advertisement { .. }),
        };
        if !source_matches_mode {
            return Vec::new();
        }

        self.readings.push_back(rssi);
        while self.readings.len() > MAX_SMOOTHING_READINGS {
            self.readings.pop_front();
        }
        self.last_reading_ms = Some(now_ms);
        if let Some(device) = self.selected.as_mut() {
            device.rssi = rssi;
        }

        // A valid, mode-matching connected reading from the selected device proves
        // the transient transport failure is over. Only `device_unavailable` is
        // cleared; unrelated stable errors survive the recovery reading.
        if matches!(event, BleLockEvent::ConnectionRead { .. })
            && self
                .last_error
                .as_ref()
                .is_some_and(|error| error.code == "device_unavailable")
        {
            self.last_error = None;
        }

        let mut effects = Vec::new();
        let average = self.average().unwrap_or(rssi);
        if self.is_close(average) {
            // A valid recovery resolves the pending episode and cancels the
            // departure timer.
            self.below_since_ms = None;
            self.lock_emitted = false;
            self.lost_emitted = false;
            self.presence = BlePresence::Near;
            if self.should_unlock(average) {
                self.unlock_armed = false;
                effects.push(BleLockEffect::Unlock);
            }
        } else if self.below_since_ms.is_none() {
            // Start the departure episode; presence stays as-is until the
            // delay elapses in `tick`.
            self.below_since_ms = Some(now_ms);
        }

        effects
    }

    /// Advance time and return any resulting intents (away/lost transitions).
    pub fn tick(&mut self, now_ms: u64) -> Vec<BleLockEffect> {
        if self.selected.is_none() {
            return Vec::new();
        }

        let mut effects = Vec::new();

        if let Some(since) = self.below_since_ms {
            let delay_ms = u64::from(self.config.lock_delay_secs) * 1_000;
            if now_ms.saturating_sub(since) >= delay_ms && !self.lock_emitted && !self.lost_emitted
            {
                if self.config.lock_rssi.is_some() {
                    effects.push(BleLockEffect::Lock);
                }
                self.presence = BlePresence::Away;
                self.unlock_armed = true;
                self.lock_emitted = true;
            }
        }

        if let Some(last) = self.last_reading_ms {
            let timeout_ms = u64::from(self.config.no_signal_timeout_secs) * 1_000;
            if now_ms.saturating_sub(last) >= timeout_ms && !self.lost_emitted {
                // One lock per away/lost episode: only emit when no lock has
                // fired yet, but still arm unlock and report lost.
                if !self.lock_emitted && self.config.lock_rssi.is_some() {
                    effects.push(BleLockEffect::Lock);
                }
                self.presence = BlePresence::Lost;
                self.unlock_armed = true;
                self.lock_emitted = true;
                self.lost_emitted = true;
                self.below_since_ms = None;
            }
        }

        // An active selection whose transport reported the device unavailable
        // retries the connection on the periodic tick. A quick failure inside the
        // throttle window consumes the last CoreBluetooth callback, and no further
        // callback arrives until we initiate the next attempt, so the tick must
        // schedule the retry itself. It is throttled to avoid busy-looping and
        // never resets presence history or manual-lock eligibility.
        if self.should_retry_connect(now_ms) {
            self.last_connect_retry_ms = Some(now_ms);
            effects.push(BleLockEffect::Connect);
        }

        effects
    }

    /// Lock immediately, independent of proximity.
    ///
    /// The still-present device cannot unlock until an away/lost observation is
    /// followed by a new near transition, because the unlock path is disarmed.
    pub fn manual_lock(&mut self, _now_ms: u64) -> Vec<BleLockEffect> {
        self.unlock_armed = false;
        vec![BleLockEffect::Lock]
    }

    /// Current status snapshot.
    pub fn status(&self) -> BleLockStatus {
        BleLockStatus {
            platform_available: cfg!(target_os = "macos"),
            bluetooth_state: self.bluetooth_state,
            monitoring: self.selected.is_some(),
            scanning: self.scanning,
            selected_device: self.selected.clone(),
            presence: self.presence,
            averaged_rssi: self.average(),
            screen_locked: self.screen_locked,
            password_configured: self.password_configured,
            accessibility_trusted: self.accessibility_trusted,
            config: self.config.clone(),
            last_error: self.last_error.clone(),
        }
    }

    // -- Step 2 seams: external state the process-owned runtime observes ------

    /// Replace the reported Bluetooth radio state.
    pub fn set_bluetooth_state(&mut self, state: BleBluetoothState) {
        self.bluetooth_state = state;
    }

    /// Replace the reported scan flag.
    pub fn set_scanning(&mut self, scanning: bool) {
        self.scanning = scanning;
    }

    /// Replace the observed screen-lock state.
    pub fn set_screen_locked(&mut self, locked: bool) {
        self.screen_locked = locked;
    }

    /// Replace the reported Keychain "password configured" flag.
    pub fn set_password_configured(&mut self, configured: bool) {
        self.password_configured = configured;
    }

    /// Replace the reported Accessibility trust flag.
    pub fn set_accessibility_trusted(&mut self, trusted: bool) {
        self.accessibility_trusted = trusted;
    }

    /// Record the last safe error.
    pub fn set_last_error(&mut self, error: BleLockError) {
        self.last_error = Some(error);
    }

    /// Clear the last safe error.
    pub fn clear_last_error(&mut self) {
        self.last_error = None;
    }

    // -- internals -----------------------------------------------------------

    fn reset_history(&mut self) {
        self.readings.clear();
        self.presence = BlePresence::Unknown;
        self.below_since_ms = None;
        self.last_reading_ms = None;
        self.lock_emitted = false;
        self.lost_emitted = false;
        self.unlock_armed = false;
        self.last_connect_retry_ms = None;
    }

    fn average(&self) -> Option<i16> {
        if self.readings.is_empty() {
            return None;
        }
        let sum: i32 = self.readings.iter().map(|value| i32::from(*value)).sum();
        Some((sum / self.readings.len() as i32) as i16)
    }

    fn is_close(&self, average: i16) -> bool {
        match self.config.lock_rssi {
            Some(lock) => average >= lock,
            None => true,
        }
    }

    fn should_unlock(&self, average: i16) -> bool {
        if !self.unlock_armed {
            return false;
        }
        match self.config.unlock_rssi {
            Some(unlock) => average >= unlock,
            None => false,
        }
    }

    fn device_unavailable(&self) -> bool {
        self.last_error
            .as_ref()
            .is_some_and(|error| error.code == "device_unavailable")
    }

    fn should_retry_connect(&self, now_ms: u64) -> bool {
        if self.config.mode != BleLockMode::Active
            || self.bluetooth_state != BleBluetoothState::PoweredOn
            || !self.device_unavailable()
        {
            return false;
        }
        match self.last_connect_retry_ms {
            Some(last) => now_ms.saturating_sub(last) >= RECONNECT_RETRY_INTERVAL_MS,
            None => true,
        }
    }
}
