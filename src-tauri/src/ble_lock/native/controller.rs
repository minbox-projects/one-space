//! OS-independent native action policy and controller.

use serde::{Deserialize, Serialize};

use crate::ble_lock::{BleBluetoothState, BleLockConfig, BleLockError, BlePresence};

/// Which Keychain service namespace a build profile owns.
///
/// The two services can never read each other's generic-password items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BleLockKeychainProfile {
    /// Release builds use `com.onespace.desktop.ble-lock`.
    Release,
    /// Debug builds use `com.onespace.desktop.ble-lock.dev`.
    Debug,
}

impl BleLockKeychainProfile {
    /// The frozen Keychain service name for this profile.
    pub fn service(self) -> &'static str {
        match self {
            BleLockKeychainProfile::Release => "com.onespace.desktop.ble-lock",
            BleLockKeychainProfile::Debug => "com.onespace.desktop.ble-lock.dev",
        }
    }
}

/// A local, non-synchronizable generic-password Keychain item description.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BleLockKeychainItem {
    /// Keychain service name.
    pub service: String,
    /// Keychain account name (the current OS user).
    pub account: String,
    /// Whether the item may sync to iCloud. Always `false` for the login password.
    pub synchronizable: bool,
}

/// External OS capability seam for the native controller.
///
/// The production implementation talks to the real macOS Keychain, lock,
/// Accessibility, display and media capabilities. Behavior tests inject an
/// independent fake OS through this trait instead of mocking internals.
pub trait BleLockPlatform {
    /// The current OS user account name.
    fn current_user_account(&self) -> String;
    /// Read a generic-password item; `Ok(None)` means "not configured".
    fn keychain_read(&self, item: &BleLockKeychainItem) -> Result<Option<String>, BleLockError>;
    /// Create or replace a generic-password item.
    fn keychain_write(
        &self,
        item: &BleLockKeychainItem,
        password: &str,
    ) -> Result<(), BleLockError>;
    /// Delete a generic-password item; a missing item is not an error.
    fn keychain_delete(&self, item: &BleLockKeychainItem) -> Result<(), BleLockError>;
    /// Whether Accessibility (assistive control) trust is granted.
    fn accessibility_trusted(&self) -> bool;
    /// Whether the current ordinary console user session is observed locked.
    fn is_session_locked(&self) -> bool;
    /// Request an ordinary session lock.
    fn lock_session(&self) -> Result<(), BleLockError>;
    /// Launch the screensaver lock path.
    fn launch_screensaver(&self) -> Result<(), BleLockError>;
    /// Best-effort display-off.
    fn sleep_display(&self) -> Result<(), BleLockError>;
    /// Best-effort display wake.
    fn wake_display(&self) -> Result<(), BleLockError>;
    /// Inject a Unicode password followed by Return once.
    fn inject_password(&self, password: &str) -> Result<(), BleLockError>;
    /// Confirmed playback state; `Err` when MediaRemote is unavailable.
    fn media_playing(&self) -> Result<bool, BleLockError>;
    /// Send an explicit pause.
    fn media_pause(&self) -> Result<(), BleLockError>;
    /// Send an explicit play.
    fn media_play(&self) -> Result<(), BleLockError>;
}

/// Public booleans reporting what a single native action attempted/observed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BleLockNativeOutcome {
    /// A lock action was requested (manual, or a qualifying presence lock).
    pub lock_requested: bool,
    /// The session was observed locked after the request.
    pub locked: bool,
    /// The display was turned off after an observed ordinary lock.
    pub display_slept: bool,
    /// The display wake was attempted and succeeded.
    pub display_woke: bool,
    /// Exactly one password injection attempt was made.
    pub password_injected: bool,
    /// Confirmed playing media received an explicit pause.
    pub media_paused: bool,
    /// Media paused in the current cycle received an explicit play.
    pub media_resumed: bool,
    /// Last safe error, if any; never carries a secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<BleLockError>,
}

/// Serializable, secret-free view of the controller runtime state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BleLockNativeSnapshot {
    /// The controller has been shut down and issues no further native action.
    pub stopped: bool,
    /// A device selection is configured.
    pub monitoring: bool,
    /// The current ordinary session is observed locked.
    pub screen_locked: bool,
    /// A Keychain password is configured (never the password itself).
    pub password_configured: bool,
    /// Accessibility trust is granted.
    pub accessibility_trusted: bool,
    /// Proximity decisions are suspended by an observed system sleep.
    pub system_sleeping: bool,
    /// Media paused in the current lock cycle is pending a matching resume.
    pub media_paused_this_cycle: bool,
    /// Last safe error, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<BleLockError>,
}

/// Native controller: the single behavioral boundary for lock/unlock/wake/media
/// actions, Keychain password operations and sleep/shutdown lifecycle.
#[derive(Clone, Debug)]
pub struct BleLockNativeController<P: BleLockPlatform> {
    config: BleLockConfig,
    profile: BleLockKeychainProfile,
    platform: P,
    stopped: bool,
    system_sleeping: bool,
    /// Unlock is armed only after an observed away/lost transition.
    unlock_armed: bool,
    /// Exactly one injection is allowed per arming episode.
    injection_done_this_episode: bool,
    /// Whether this lock cycle recorded a media pause awaiting resume.
    media_paused_this_cycle: bool,
    /// Ordinary lock observed requested but not yet confirmed (display-off waits).
    pending_display_off: bool,
    /// Last presence passed to [`Self::observe_presence`], for transition edges.
    last_observed_presence: BlePresence,
    last_error: Option<BleLockError>,
}

impl<P: BleLockPlatform> BleLockNativeController<P> {
    /// Create a controller from a validated configuration, profile and platform.
    pub fn new(config: BleLockConfig, profile: BleLockKeychainProfile, platform: P) -> Self {
        Self {
            config,
            profile,
            platform,
            stopped: false,
            system_sleeping: false,
            unlock_armed: false,
            injection_done_this_episode: false,
            media_paused_this_cycle: false,
            pending_display_off: false,
            last_observed_presence: BlePresence::Unknown,
            last_error: None,
        }
    }

    /// The active configuration.
    pub fn config(&self) -> &BleLockConfig {
        &self.config
    }

    /// The Keychain profile in use.
    pub fn profile(&self) -> BleLockKeychainProfile {
        self.profile
    }

    /// The platform seam handle.
    pub fn platform(&self) -> &P {
        &self.platform
    }

    /// The current-user, non-synchronizable generic-password item for this profile.
    pub fn keychain_item(&self) -> BleLockKeychainItem {
        BleLockKeychainItem {
            service: self.profile.service().to_string(),
            account: self.platform.current_user_account(),
            synchronizable: false,
        }
    }

    /// Whether a Keychain password is configured; only a boolean is exposed.
    pub fn password_configured(&self) -> bool {
        matches!(
            self.platform.keychain_read(&self.keychain_item()),
            Ok(Some(_))
        )
    }

    /// Store the login password in the profile-local Keychain item.
    pub fn set_password(&mut self, password: &str) -> Result<(), BleLockError> {
        let item = self.keychain_item();
        self.platform.keychain_write(&item, password)
    }

    /// Delete the profile-local Keychain item.
    pub fn delete_password(&mut self) -> Result<(), BleLockError> {
        let item = self.keychain_item();
        self.platform.keychain_delete(&item)
    }

    /// Lock immediately, independent of proximity, and disarm proximity unlock.
    pub fn lock_now(&mut self) -> BleLockNativeOutcome {
        let mut outcome = BleLockNativeOutcome::default();
        if self.stopped {
            return outcome;
        }
        outcome.lock_requested = true;
        // A manual lock disarms proximity unlock until a fresh away/lost
        // transition is observed.
        self.unlock_armed = false;
        self.injection_done_this_episode = true;
        self.perform_lock(&mut outcome);
        self.pause_media_for_cycle(&mut outcome);
        outcome
    }

    /// Record an automatic away/lost transition: arm proximity unlock.
    pub fn note_away_or_lost(&mut self) {
        self.unlock_armed = true;
        self.injection_done_this_episode = false;
    }

    /// Record a near transition (no measurement): wake the display and, when
    /// fully qualified, make at most one guarded password injection.
    #[cfg(test)]
    pub fn note_near(&mut self) -> BleLockNativeOutcome {
        let mut outcome = BleLockNativeOutcome::default();
        if self.stopped || self.system_sleeping {
            return outcome;
        }
        self.wake_for_near(&mut outcome);
        if self.unlock_armed_for_action() {
            self.attempt_unlock(&mut outcome);
        }
        outcome
    }

    /// Production presence adapter: translate an observed presence plus the
    /// smoothed RSSI mean into native policy.
    ///
    /// The monitor keeps presence `Near` across readings, so injection
    /// eligibility must come from the mean crossing the unlock threshold, not
    /// from a mere near reading. The away/lost edge arms exactly one eligibility;
    /// a repeated reading never re-arms or re-injects, and the display wakes only
    /// on the near edge.
    pub fn observe_presence(
        &mut self,
        presence: BlePresence,
        averaged_rssi: Option<i16>,
    ) -> BleLockNativeOutcome {
        let mut outcome = BleLockNativeOutcome::default();
        if self.stopped || self.system_sleeping {
            return outcome;
        }
        let entered_near =
            presence == BlePresence::Near && self.last_observed_presence != BlePresence::Near;
        self.last_observed_presence = presence;
        match presence {
            BlePresence::Away | BlePresence::Lost => {
                // Reuse the tested arming policy so a repeated away/lost reading
                // re-arms at most the same single eligibility.
                self.note_away_or_lost();
            }
            BlePresence::Near => {
                if entered_near {
                    self.wake_for_near(&mut outcome);
                }
                if self.unlock_armed_for_action() {
                    if let Some(mean) = averaged_rssi {
                        if self.mean_clears_unlock(mean) {
                            self.attempt_unlock(&mut outcome);
                        }
                    }
                }
            }
            BlePresence::Unknown => {}
        }
        outcome
    }

    /// Poll the observed native state: resume media exactly once after an
    /// asynchronous/manual unlock, and perform a display-off whose ordinary lock
    /// was only observed on a later poll.
    pub fn poll_native_state(&mut self) -> BleLockNativeOutcome {
        let mut outcome = BleLockNativeOutcome::default();
        if self.stopped || self.system_sleeping {
            return outcome;
        }
        self.resume_media_after_observed_unlock(&mut outcome);
        if self.pending_display_off
            && !self.config.use_screensaver_to_lock
            && self.config.sleep_display
            && self.platform.is_session_locked()
        {
            match self.platform.sleep_display() {
                Ok(()) => {
                    self.pending_display_off = false;
                    outcome.display_slept = true;
                }
                Err(error) => self.record_error(&mut outcome, error),
            }
        }
        outcome
    }

    /// Record an observed system sleep: suppress proximity decisions and any
    /// pending display-off, and publish the stable `system_sleep` status.
    pub fn note_system_sleep(&mut self) {
        self.system_sleeping = true;
        self.pending_display_off = false;
        self.last_error = Some(BleLockError::new("system_sleep", None));
    }

    /// Record an observed system wake, permitting a rebuild only while running.
    pub fn note_system_wake(&mut self) -> bool {
        if self.stopped {
            return false;
        }
        self.system_sleeping = false;
        self.unlock_armed = false;
        self.injection_done_this_episode = true;
        self.last_observed_presence = BlePresence::Unknown;
        if self
            .last_error
            .as_ref()
            .map(|error| error.code == "system_sleep")
            .unwrap_or(false)
        {
            self.last_error = None;
        }
        true
    }

    /// Report radio capability failures and clear only radio errors on recovery.
    pub fn observe_bluetooth_state(&mut self, state: BleBluetoothState) {
        match state {
            BleBluetoothState::Unsupported => {
                self.last_error = Some(BleLockError::new("bluetooth_unsupported", None));
            }
            BleBluetoothState::PoweredOff => {
                self.last_error = Some(BleLockError::bluetooth_powered_off());
            }
            BleBluetoothState::Unauthorized => {
                self.last_error = Some(BleLockError::bluetooth_unauthorized());
            }
            BleBluetoothState::PoweredOn => {
                if self.last_error.as_ref().is_some_and(|error| {
                    matches!(
                        error.code.as_str(),
                        "bluetooth_unsupported"
                            | "bluetooth_powered_off"
                            | "bluetooth_unauthorized"
                    )
                }) {
                    self.last_error = None;
                }
            }
            _ => {}
        }
    }

    /// Stop the controller. Idempotent; issues no further native actions.
    pub fn shutdown(&mut self) {
        self.stopped = true;
    }

    /// Snapshot the current controller state (never contains a password).
    pub fn snapshot(&self) -> BleLockNativeSnapshot {
        // Query once per snapshot: a read fault is not the same as an absent
        // password. Never propagate the platform's error detail or credentials.
        let (password_configured, last_error) =
            match self.platform.keychain_read(&self.keychain_item()) {
                Ok(password) => (
                    password.is_some(),
                    self.last_error
                        .as_ref()
                        .filter(|error| error.code != "keychain_unavailable")
                        .cloned(),
                ),
                Err(_) => (false, Some(BleLockError::new("keychain_unavailable", None))),
            };
        BleLockNativeSnapshot {
            stopped: self.stopped,
            monitoring: self.config.device_id.is_some(),
            screen_locked: self.platform.is_session_locked(),
            password_configured,
            accessibility_trusted: self.platform.accessibility_trusted(),
            system_sleeping: self.system_sleeping,
            media_paused_this_cycle: self.media_paused_this_cycle,
            last_error,
        }
    }

    /// Replace the active configuration.
    ///
    /// A different selected device id clears all prior arming/cycle state so a
    /// replaced or newly selected device can never inherit eligibility; a
    /// same-device edit preserves the manual-lock guard.
    pub fn set_config(&mut self, config: BleLockConfig) {
        let device_changed = self.config.device_id != config.device_id;
        self.config = config;
        if device_changed {
            self.unlock_armed = false;
            self.injection_done_this_episode = true;
            self.media_paused_this_cycle = false;
            self.pending_display_off = false;
            self.last_observed_presence = BlePresence::Unknown;
        }
    }

    /// Whether proximity decisions are suspended by an observed system sleep.
    pub fn is_system_sleeping(&self) -> bool {
        self.system_sleeping
    }

    // -- internals -----------------------------------------------------------

    fn unlock_armed_for_action(&self) -> bool {
        self.unlock_armed
            && !self.injection_done_this_episode
            && self.config.unlock_rssi.is_some()
            && !self.config.wake_without_unlocking
    }

    /// The unlock threshold is authoritative when automatic unlock is enabled;
    /// there is no distance inference and no wake-only unlock.
    fn mean_clears_unlock(&self, mean: i16) -> bool {
        match self.config.unlock_rssi {
            Some(unlock) => mean >= unlock,
            None => false,
        }
    }

    fn wake_for_near(&mut self, outcome: &mut BleLockNativeOutcome) {
        if !self.config.wake_on_proximity {
            return;
        }
        match self.platform.wake_display() {
            Ok(()) => outcome.display_woke = true,
            Err(error) => self.record_error(outcome, error),
        }
    }

    /// One guarded unlock attempt against the qualifying gates.
    fn attempt_unlock(&mut self, outcome: &mut BleLockNativeOutcome) {
        if !self.platform.accessibility_trusted() {
            self.record_error(outcome, BleLockError::new("accessibility_required", None));
            return;
        }
        if !self.platform.is_session_locked() {
            // Not the current ordinary locked session; nothing to inject.
            return;
        }
        match self.platform.keychain_read(&self.keychain_item()) {
            Ok(Some(password)) => {
                self.injection_done_this_episode = true;
                self.unlock_armed = false;
                match self.platform.inject_password(&password) {
                    Ok(()) => {
                        outcome.password_injected = true;
                        self.resume_media_after_observed_unlock(outcome);
                    }
                    Err(error) => self.record_error(outcome, error),
                }
            }
            Ok(None) => {
                self.record_error(outcome, BleLockError::new("password_not_configured", None))
            }
            Err(error) => self.record_error(outcome, error),
        }
    }

    fn record_error(&mut self, outcome: &mut BleLockNativeOutcome, error: BleLockError) {
        // Propagate the stable, non-secret warning into the returned outcome as
        // well as the controller snapshot; it must survive a following lock.
        self.last_error = Some(error.clone());
        outcome.last_error = Some(error);
    }

    fn perform_lock(&mut self, outcome: &mut BleLockNativeOutcome) {
        if self.config.use_screensaver_to_lock {
            if let Err(error) = self.platform.launch_screensaver() {
                self.record_error(outcome, error);
            }
        } else if let Err(error) = self.platform.lock_session() {
            self.record_error(outcome, error);
        }
        // Screensaver lock and display-off are mutually exclusive.
        outcome.locked = self.platform.is_session_locked();
        if !self.config.use_screensaver_to_lock && self.config.sleep_display {
            if outcome.locked {
                match self.platform.sleep_display() {
                    Ok(()) => outcome.display_slept = true,
                    Err(error) => self.record_error(outcome, error),
                }
            } else {
                // The ordinary lock was requested but not yet observed; a later
                // poll performs the display-off only after the real lock.
                self.pending_display_off = true;
            }
        }
    }

    fn pause_media_for_cycle(&mut self, outcome: &mut BleLockNativeOutcome) {
        if !self.config.pause_now_playing {
            return;
        }
        match self.platform.media_playing() {
            Ok(true) => match self.platform.media_pause() {
                Ok(()) => {
                    outcome.media_paused = true;
                    self.media_paused_this_cycle = true;
                }
                Err(error) => self.record_error(outcome, error),
            },
            Ok(false) => {}
            Err(error) => self.record_error(outcome, error),
        }
    }

    fn resume_media_after_observed_unlock(&mut self, outcome: &mut BleLockNativeOutcome) {
        if !self.media_paused_this_cycle || self.platform.is_session_locked() {
            return;
        }
        match self.platform.media_play() {
            Ok(()) => {
                self.media_paused_this_cycle = false;
                outcome.media_resumed = true;
            }
            Err(error) => self.record_error(outcome, error),
        }
    }

    /// Perform a lock triggered by a real away/lost presence transition.
    ///
    /// Unlike the manual [`Self::lock_now`] it never disarms proximity unlock;
    /// the runtime arms through [`Self::note_away_or_lost`] from the observed
    /// transition, so a repeated tick cannot re-lock.
    pub fn lock_for_presence(&mut self) -> BleLockNativeOutcome {
        let mut outcome = BleLockNativeOutcome::default();
        if self.stopped || self.system_sleeping {
            return outcome;
        }
        outcome.lock_requested = true;
        self.perform_lock(&mut outcome);
        self.pause_media_for_cycle(&mut outcome);
        outcome
    }
}
