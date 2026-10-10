//! Native action policy/controller for the `ble-lock` tool (frozen plan Step 2).
//!
//! This module is the single OS-controller behavioral boundary for
//! `Lock Now` / proximity transitions / password operations / sleep-wake /
//! cleanup. It is deliberately written against an injected external OS
//! capability seam ([`BleLockPlatform`]) so behavior tests can supply an
//! independent fake OS (observed session state, Keychain availability/content,
//! Accessibility trust, media playing state and a captured command log) instead
//! of mocking internal collaborators.
//!
//! The product implementation — the controller, the real macOS
//! [`BleLockPlatform`] bridge (frozen Keychain services
//! `com.onespace.desktop.ble-lock` and `com.onespace.desktop.ble-lock.dev`, the
//! observed ordinary-session lock, Accessibility, display, one-shot CGEvent
//! injection and explicit MediaRemote control) and the process-owned runtime with
//! its 10 IPC commands and two events — are the top-level items of this file,
//! above the delegated `#[cfg(test)]` behavior tests. The file is compiled into
//! both product and test builds.
//!
//! The controller types are resolved from this file's own module boundary
//! (`super::`), not a fixed `crate::ble_lock::native` path, because the native
//! Rust module path is not frozen. The sole registration path is the `#[path]`
//! include in `app_runtime/run_app.rs` (`pub(crate) mod ble_lock_native;`), which
//! is always loaded; no `ble_lock.rs` change is required.
//!
//! The tests assert stable action outcomes and public snapshot/flags through the
//! OS seam; they never inspect private functions or code shape, and they never
//! assert on constants alone.

use serde::{Deserialize, Serialize};

use crate::ble_lock::{BleLockConfig, BleLockError};

/// Which Keychain service namespace a build profile owns.
///
/// The two services can never read each other's generic-password items
/// (REQ-006/AC-008/AC-015).
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
        // transition is observed (REQ-007/AC-010).
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

    /// Record an observed system wake: rebuild intent until a fresh away/lost.
    pub fn note_system_wake(&mut self) {
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
    }

    /// Stop the controller. Idempotent; issues no further native actions.
    pub fn shutdown(&mut self) {
        self.stopped = true;
    }

    /// Snapshot the current controller state (never contains a password).
    pub fn snapshot(&self) -> BleLockNativeSnapshot {
        BleLockNativeSnapshot {
            stopped: self.stopped,
            monitoring: self.config.device_id.is_some(),
            screen_locked: self.platform.is_session_locked(),
            password_configured: self.password_configured(),
            accessibility_trusted: self.platform.accessibility_trusted(),
            system_sleeping: self.system_sleeping,
            media_paused_this_cycle: self.media_paused_this_cycle,
            last_error: self.last_error.clone(),
        }
    }

    /// Replace the active configuration.
    ///
    /// A different selected device id clears all prior arming/cycle state so a
    /// replaced or newly selected device can never inherit eligibility; a
    /// same-device edit preserves the manual-lock guard (REQ-007/AC-004).
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

    /// One guarded unlock attempt against the qualifying gates (AC-009/AC-017).
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
        // Screensaver lock and display-off are mutually exclusive (REQ-010).
        outcome.locked = self.platform.is_session_locked();
        if !self.config.use_screensaver_to_lock && self.config.sleep_display {
            if outcome.locked {
                match self.platform.sleep_display() {
                    Ok(()) => outcome.display_slept = true,
                    Err(error) => self.record_error(outcome, error),
                }
            } else {
                // The ordinary lock was requested but not yet observed; a later
                // poll performs the display-off only after the real lock (AC-013).
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

/// Real macOS platform implementation using system frameworks through raw FFI.
#[cfg(target_os = "macos")]
mod mac {
    use super::{BleLockKeychainItem, BleLockPlatform};
    use crate::ble_lock::BleLockError;
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_int, c_void};
    use std::ptr;
    use std::sync::OnceLock;
    use std::time::Duration;

    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFDataRef = *const c_void;
    type CFAllocatorRef = *const c_void;
    type CFDictionaryRef = *const c_void;
    type CFIndex = isize;
    type Boolean = u8;
    type OSStatus = i32;

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    /// `kCFNumberSInt32Type`.
    const K_CF_NUMBER_SINT32: c_int = 3;
    const ERR_SEC_SUCCESS: OSStatus = 0;
    const ERR_SEC_ITEM_NOT_FOUND: OSStatus = -25300;

    fn keychain_error() -> BleLockError {
        // Stable code only; never echoes a password or OS detail.
        BleLockError::new("keychain_unavailable", None)
    }

    fn native_unavailable() -> BleLockError {
        BleLockError::new("native_action_unavailable", None)
    }

    fn media_unavailable() -> BleLockError {
        BleLockError::new("media_control_unavailable", None)
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFBooleanTrue: CFTypeRef;
        static kCFBooleanFalse: CFTypeRef;
        static kCFTypeDictionaryKeyCallBacks: c_void;
        static kCFTypeDictionaryValueCallBacks: c_void;
        fn CFStringCreateWithBytes(
            alloc: CFAllocatorRef,
            bytes: *const u8,
            len: CFIndex,
            encoding: u32,
            is_external: Boolean,
        ) -> CFStringRef;
        fn CFDataCreate(alloc: CFAllocatorRef, bytes: *const u8, len: CFIndex) -> CFDataRef;
        fn CFDataGetBytePtr(data: CFDataRef) -> *const u8;
        fn CFDataGetLength(data: CFDataRef) -> CFIndex;
        fn CFDictionaryCreate(
            alloc: CFAllocatorRef,
            keys: *const *const c_void,
            values: *const *const c_void,
            count: CFIndex,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> CFDictionaryRef;
        fn CFDictionaryGetValue(dict: CFDictionaryRef, key: *const c_void) -> *const c_void;
        fn CFBooleanGetValue(value: CFTypeRef) -> Boolean;
        fn CFNumberGetValue(number: CFTypeRef, the_type: c_int, value: *mut c_void) -> Boolean;
        fn CFRelease(cf: CFTypeRef);
    }

    unsafe extern "C" {
        fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dispatch_queue_create(label: *const c_char, attr: *const c_void) -> *mut c_void;
        fn getuid() -> u32;
        fn getpwuid_r(
            uid: u32,
            pwd: *mut Passwd,
            buf: *mut c_char,
            buflen: usize,
            result: *mut *mut Passwd,
        ) -> c_int;
    }

    /// Minimal `struct passwd` prefix sufficient for `pw_name`.
    #[repr(C)]
    struct Passwd {
        pw_name: *mut c_char,
        pw_passwd: *mut c_char,
        pw_uid: u32,
        pw_gid: u32,
        pw_gecos: *mut c_char,
        pw_dir: *mut c_char,
        pw_shell: *mut c_char,
    }

    #[link(name = "Security", kind = "framework")]
    extern "C" {
        static kSecClass: CFStringRef;
        static kSecClassGenericPassword: CFStringRef;
        static kSecAttrService: CFStringRef;
        static kSecAttrAccount: CFStringRef;
        static kSecAttrSynchronizable: CFStringRef;
        static kSecValueData: CFStringRef;
        static kSecReturnData: CFStringRef;
        fn SecItemAdd(attributes: CFDictionaryRef, result: *mut CFTypeRef) -> OSStatus;
        fn SecItemCopyMatching(query: CFDictionaryRef, result: *mut CFTypeRef) -> OSStatus;
        fn SecItemUpdate(query: CFDictionaryRef, attributes: CFDictionaryRef) -> OSStatus;
        fn SecItemDelete(query: CFDictionaryRef) -> OSStatus;
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> Boolean;
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> Boolean;
        static kAXTrustedCheckOptionPrompt: CFStringRef;
        fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
    }

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPMAssertionDeclareUserActivity(
            assertion_name: CFStringRef,
            user_type: u32,
            assertion_id: *mut u32,
        ) -> i32;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventCreateKeyboardEvent(
            source: *const c_void,
            virtual_key: u16,
            key_down: bool,
        ) -> *mut c_void;
        fn CGEventKeyboardSetUnicodeString(event: *mut c_void, length: usize, string: *const u16);
        fn CGEventPost(tap: u32, event: *mut c_void);
    }

    type GetPlayingFn = unsafe extern "C" fn(*mut c_void, *mut c_void);
    type SendCommandFn = unsafe extern "C" fn(c_int, *const c_void) -> Boolean;

    const MEDIAREMOTE_PATH: &[u8] =
        b"/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote\0";
    const LOGIN_FRAMEWORK_PATH: &[u8] =
        b"/System/Library/PrivateFrameworks/login.framework/login\0";
    const RTLD_LAZY: c_int = 0x1;

    fn cf_string(value: &str) -> CFStringRef {
        unsafe {
            CFStringCreateWithBytes(
                ptr::null(),
                value.as_ptr(),
                value.len() as CFIndex,
                K_CF_STRING_ENCODING_UTF8,
                0,
            )
        }
    }

    unsafe fn open_symbol(path: &[u8], symbol: &[u8]) -> Option<*mut c_void> {
        let handle = dlopen(path.as_ptr() as *const c_char, RTLD_LAZY);
        if handle.is_null() {
            return None;
        }
        let symbol = dlsym(handle, symbol.as_ptr() as *const c_char);
        if symbol.is_null() {
            None
        } else {
            Some(symbol)
        }
    }

    /// Build an owned CFDictionary from borrowed keys/values (caller releases).
    fn cf_dictionary(pairs: &[(*const c_void, *const c_void)]) -> Option<CFDictionaryRef> {
        unsafe {
            let keys: Vec<*const c_void> = pairs.iter().map(|(key, _)| *key).collect();
            let values: Vec<*const c_void> = pairs.iter().map(|(_, value)| *value).collect();
            let dictionary = CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                pairs.len() as CFIndex,
                &kCFTypeDictionaryKeyCallBacks as *const c_void,
                &kCFTypeDictionaryValueCallBacks as *const c_void,
            );
            if dictionary.is_null() {
                None
            } else {
                Some(dictionary)
            }
        }
    }

    fn keychain_query(
        item: &BleLockKeychainItem,
        include_value: Option<&str>,
    ) -> Option<(CFDictionaryRef, Vec<CFTypeRef>)> {
        unsafe {
            let service = cf_string(&item.service);
            let account = cf_string(&item.account);
            if service.is_null() || account.is_null() {
                if !service.is_null() {
                    CFRelease(service);
                }
                if !account.is_null() {
                    CFRelease(account);
                }
                return None;
            }
            let mut owned: Vec<CFTypeRef> = vec![service, account];
            let data = match include_value {
                Some(value) => {
                    let data = CFDataCreate(ptr::null(), value.as_ptr(), value.len() as CFIndex);
                    if data.is_null() {
                        for value in owned {
                            CFRelease(value);
                        }
                        return None;
                    }
                    owned.push(data);
                    data
                }
                None => ptr::null(),
            };

            let mut keys: Vec<*const c_void> = vec![
                kSecClass,
                kSecAttrService,
                kSecAttrAccount,
                kSecAttrSynchronizable,
            ];
            let mut values: Vec<*const c_void> =
                vec![kSecClassGenericPassword, service, account, kCFBooleanFalse];
            if include_value.is_some() {
                keys.push(kSecValueData);
                values.push(data);
            }
            let dictionary = CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                keys.len() as CFIndex,
                &kCFTypeDictionaryKeyCallBacks as *const c_void,
                &kCFTypeDictionaryValueCallBacks as *const c_void,
            );
            if dictionary.is_null() {
                for value in owned {
                    CFRelease(value);
                }
                return None;
            }
            Some((dictionary, owned))
        }
    }

    fn keychain_read(item: &BleLockKeychainItem) -> Result<Option<String>, BleLockError> {
        unsafe {
            let service = cf_string(&item.service);
            let account = cf_string(&item.account);
            if service.is_null() || account.is_null() {
                if !service.is_null() {
                    CFRelease(service);
                }
                if !account.is_null() {
                    CFRelease(account);
                }
                return Err(keychain_error());
            }
            let keys: [*const c_void; 5] = [
                kSecClass,
                kSecAttrService,
                kSecAttrAccount,
                kSecAttrSynchronizable,
                kSecReturnData,
            ];
            let values: [*const c_void; 5] = [
                kSecClassGenericPassword,
                service,
                account,
                kCFBooleanFalse,
                kCFBooleanTrue,
            ];
            let query = CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                5,
                &kCFTypeDictionaryKeyCallBacks as *const c_void,
                &kCFTypeDictionaryValueCallBacks as *const c_void,
            );
            let mut result: CFTypeRef = ptr::null();
            let status = if query.is_null() {
                -1
            } else {
                SecItemCopyMatching(query, &mut result)
            };
            if !query.is_null() {
                CFRelease(query);
            }
            CFRelease(service);
            CFRelease(account);
            if status == ERR_SEC_ITEM_NOT_FOUND {
                return Ok(None);
            }
            if status != ERR_SEC_SUCCESS || result.is_null() {
                if !result.is_null() {
                    CFRelease(result);
                }
                return Err(keychain_error());
            }
            let data = result as CFDataRef;
            let length = CFDataGetLength(data);
            let bytes = CFDataGetBytePtr(data);
            let value = if bytes.is_null() || length < 0 {
                Err(keychain_error())
            } else {
                let slice = std::slice::from_raw_parts(bytes, length as usize);
                String::from_utf8(slice.to_vec()).map_err(|_| keychain_error())
            };
            CFRelease(result);
            value.map(Some)
        }
    }

    fn keychain_write(item: &BleLockKeychainItem, password: &str) -> Result<(), BleLockError> {
        // Replace with `SecItemUpdate`, adding only when the item is absent, so
        // repeated saves never delete the existing item before success.
        unsafe {
            let service = cf_string(&item.service);
            let account = cf_string(&item.account);
            let data = CFDataCreate(ptr::null(), password.as_ptr(), password.len() as CFIndex);
            if service.is_null() || account.is_null() || data.is_null() {
                if !service.is_null() {
                    CFRelease(service);
                }
                if !account.is_null() {
                    CFRelease(account);
                }
                if !data.is_null() {
                    CFRelease(data);
                }
                return Err(keychain_error());
            }
            let owned = [service, account, data];

            let query = cf_dictionary(&[
                (kSecClass, kSecClassGenericPassword),
                (kSecAttrService, service),
                (kSecAttrAccount, account),
                (kSecAttrSynchronizable, kCFBooleanFalse),
            ]);
            let attributes = cf_dictionary(&[(kSecValueData, data)]);
            let update_status = match (query, attributes) {
                (Some(query), Some(attributes)) => {
                    let status = SecItemUpdate(query, attributes);
                    CFRelease(query);
                    CFRelease(attributes);
                    status
                }
                _ => -1,
            };
            if update_status == ERR_SEC_SUCCESS {
                for value in owned {
                    CFRelease(value);
                }
                return Ok(());
            }

            let add = cf_dictionary(&[
                (kSecClass, kSecClassGenericPassword),
                (kSecAttrService, service),
                (kSecAttrAccount, account),
                (kSecAttrSynchronizable, kCFBooleanFalse),
                (kSecValueData, data),
            ]);
            let add_status = if let Some(add) = add {
                let mut result: CFTypeRef = ptr::null();
                let status = SecItemAdd(add, &mut result);
                if !result.is_null() {
                    CFRelease(result);
                }
                CFRelease(add);
                status
            } else {
                -1
            };
            for value in owned {
                CFRelease(value);
            }
            if add_status == ERR_SEC_SUCCESS {
                Ok(())
            } else {
                Err(keychain_error())
            }
        }
    }

    fn keychain_delete(item: &BleLockKeychainItem) -> Result<(), BleLockError> {
        unsafe {
            let Some((query, owned)) = keychain_query(item, None) else {
                return Err(keychain_error());
            };
            let status = SecItemDelete(query);
            CFRelease(query);
            for value in owned {
                CFRelease(value);
            }
            if status == ERR_SEC_SUCCESS || status == ERR_SEC_ITEM_NOT_FOUND {
                Ok(())
            } else {
                Err(keychain_error())
            }
        }
    }

    /// The real OS user account name for this process (never `$USER`).
    fn current_user_account_name() -> String {
        unsafe {
            let uid = getuid();
            let mut passwd: Passwd = std::mem::zeroed();
            let mut buffer = [0 as c_char; 1024];
            let mut result: *mut Passwd = ptr::null_mut();
            if getpwuid_r(
                uid,
                &mut passwd,
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut result,
            ) == 0
                && !result.is_null()
                && !passwd.pw_name.is_null()
            {
                if let Ok(text) = CStr::from_ptr(passwd.pw_name).to_str() {
                    if !text.is_empty() {
                        return text.to_string();
                    }
                }
            }
            "unknown".to_string()
        }
    }

    /// Observed lock of the *current ordinary logged-in console* session.
    ///
    /// False at loginwindow, pre-boot, for any non-console session and for a
    /// different OS user; the undocumented `CGSSessionScreenIsLocked` flag alone
    /// is never sufficient.
    fn session_locked() -> bool {
        unsafe {
            let dictionary = CGSessionCopyCurrentDictionary();
            if dictionary.is_null() {
                return false;
            }
            let on_console_key = cf_string("kCGSSessionOnConsoleKey");
            let locked_key = cf_string("CGSSessionScreenIsLocked");
            let uid_key = cf_string("kCGSSessionUserIDKey");
            let login_done_key = cf_string("kCGSessionLoginDoneKey");
            let bool_value = |key: CFStringRef| -> bool {
                if key.is_null() {
                    return false;
                }
                let value = CFDictionaryGetValue(dictionary, key);
                !value.is_null() && CFBooleanGetValue(value) != 0
            };
            let on_console = bool_value(on_console_key);
            let locked = bool_value(locked_key);
            let login_done = bool_value(login_done_key);
            let mut session_uid: c_int = -1;
            if !uid_key.is_null() {
                let value = CFDictionaryGetValue(dictionary, uid_key);
                if !value.is_null() {
                    CFNumberGetValue(
                        value,
                        K_CF_NUMBER_SINT32,
                        &mut session_uid as *mut c_int as *mut c_void,
                    );
                }
            }
            for key in [on_console_key, locked_key, uid_key, login_done_key] {
                if !key.is_null() {
                    CFRelease(key);
                }
            }
            CFRelease(dictionary);
            on_console && login_done && locked && session_uid >= 0 && session_uid as u32 == getuid()
        }
    }

    fn request_accessibility_trust() -> bool {
        unsafe {
            let key = kAXTrustedCheckOptionPrompt;
            let values: [*const c_void; 1] = [kCFBooleanTrue];
            let keys: [*const c_void; 1] = [key];
            let options = CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                &kCFTypeDictionaryKeyCallBacks as *const c_void,
                &kCFTypeDictionaryValueCallBacks as *const c_void,
            );
            if options.is_null() {
                return false;
            }
            let trusted = AXIsProcessTrustedWithOptions(options) != 0;
            CFRelease(options);
            trusted
        }
    }

    fn inject_password(password: &str) -> Result<(), BleLockError> {
        // CGEventKeyboardSetUnicodeString accepts at most 20 UTF-16 units per
        // event; never split a surrogate pair across events.
        const MAX_UNITS: usize = 20;
        let units: Vec<u16> = password.encode_utf16().collect();
        let mut index = 0;
        while index < units.len() {
            let mut end = (index + MAX_UNITS).min(units.len());
            if end < units.len() && (0xD800..=0xDBFF).contains(&units[end - 1]) {
                end -= 1;
            }
            if end <= index {
                end = (index + 1).min(units.len());
            }
            let chunk = &units[index..end];
            unsafe {
                let down = CGEventCreateKeyboardEvent(ptr::null(), 0, true);
                if down.is_null() {
                    return Err(native_unavailable());
                }
                CGEventKeyboardSetUnicodeString(down, chunk.len(), chunk.as_ptr());
                CGEventPost(0, down);
                CFRelease(down);
                let up = CGEventCreateKeyboardEvent(ptr::null(), 0, false);
                if up.is_null() {
                    return Err(native_unavailable());
                }
                CGEventPost(0, up);
                CFRelease(up);
            }
            index = end;
        }
        // Submit with one Return key press.
        unsafe {
            for key_down in [true, false] {
                let event = CGEventCreateKeyboardEvent(ptr::null(), 36, key_down);
                if event.is_null() {
                    return Err(native_unavailable());
                }
                CGEventPost(0, event);
                CFRelease(event);
            }
        }
        Ok(())
    }

    struct MediaSymbols {
        get_playing: GetPlayingFn,
        send_command: SendCommandFn,
    }

    /// Resolve the private MediaRemote capability once for the process lifetime.
    fn media_symbols() -> Option<&'static MediaSymbols> {
        static SYMBOLS: OnceLock<Option<MediaSymbols>> = OnceLock::new();
        SYMBOLS
            .get_or_init(|| unsafe {
                let handle = dlopen(MEDIAREMOTE_PATH.as_ptr() as *const c_char, RTLD_LAZY);
                if handle.is_null() {
                    return None;
                }
                let get_playing = dlsym(
                    handle,
                    b"MRMediaRemoteGetNowPlayingApplicationIsPlaying\0".as_ptr() as *const c_char,
                );
                let send_command = dlsym(
                    handle,
                    b"MRMediaRemoteSendCommand\0".as_ptr() as *const c_char,
                );
                if get_playing.is_null() || send_command.is_null() {
                    return None;
                }
                Some(MediaSymbols {
                    get_playing: std::mem::transmute(get_playing),
                    send_command: std::mem::transmute(send_command),
                })
            })
            .as_ref()
    }

    /// A single private completion queue for MediaRemote callbacks, so waiting
    /// never blocks the main queue.
    fn media_queue() -> Option<*mut c_void> {
        static QUEUE: OnceLock<usize> = OnceLock::new();
        let raw = *QUEUE.get_or_init(|| unsafe {
            dispatch_queue_create(
                b"com.onespace.ble-lock.media\0".as_ptr() as *const c_char,
                ptr::null(),
            ) as usize
        });
        if raw == 0 {
            None
        } else {
            Some(raw as *mut c_void)
        }
    }

    fn media_playing() -> Result<bool, BleLockError> {
        let Some(symbols) = media_symbols() else {
            return Err(media_unavailable());
        };
        let Some(queue) = media_queue() else {
            return Err(media_unavailable());
        };
        let (sender, receiver) = std::sync::mpsc::channel::<bool>();
        let block = block2::RcBlock::new(move |is_playing: u8| {
            let _ = sender.send(is_playing != 0);
        });
        unsafe {
            (symbols.get_playing)(queue, &*block as *const _ as *mut c_void);
        }
        match receiver.recv_timeout(Duration::from_millis(500)) {
            Ok(playing) => Ok(playing),
            Err(_) => Err(media_unavailable()),
        }
    }

    fn media_command(command: c_int) -> Result<(), BleLockError> {
        let Some(symbols) = media_symbols() else {
            return Err(media_unavailable());
        };
        if unsafe { (symbols.send_command)(command, ptr::null()) } != 0 {
            Ok(())
        } else {
            Err(media_unavailable())
        }
    }

    /// Real macOS platform.
    #[derive(Clone, Copy, Debug)]
    pub struct MacPlatform;

    impl BleLockPlatform for MacPlatform {
        fn current_user_account(&self) -> String {
            current_user_account_name()
        }

        fn keychain_read(
            &self,
            item: &BleLockKeychainItem,
        ) -> Result<Option<String>, BleLockError> {
            keychain_read(item)
        }

        fn keychain_write(
            &self,
            item: &BleLockKeychainItem,
            password: &str,
        ) -> Result<(), BleLockError> {
            keychain_write(item, password)
        }

        fn keychain_delete(&self, item: &BleLockKeychainItem) -> Result<(), BleLockError> {
            keychain_delete(item)
        }

        fn accessibility_trusted(&self) -> bool {
            unsafe { AXIsProcessTrusted() != 0 }
        }

        fn is_session_locked(&self) -> bool {
            session_locked()
        }

        fn lock_session(&self) -> Result<(), BleLockError> {
            unsafe {
                let Some(symbol) = open_symbol(LOGIN_FRAMEWORK_PATH, b"SACLockScreenImmediate\0")
                else {
                    return Err(native_unavailable());
                };
                // Private login framework symbol is `void SACLockScreenImmediate(void)`.
                let lock: unsafe extern "C" fn() = std::mem::transmute(symbol);
                lock();
                Ok(())
            }
        }

        fn launch_screensaver(&self) -> Result<(), BleLockError> {
            let status = std::process::Command::new("/usr/bin/open")
                .args(["-a", "ScreenSaverEngine"])
                .status()
                .map_err(|_| native_unavailable())?;
            if status.success() {
                Ok(())
            } else {
                Err(native_unavailable())
            }
        }

        fn sleep_display(&self) -> Result<(), BleLockError> {
            let status = std::process::Command::new("/usr/bin/pmset")
                .arg("displaysleepnow")
                .status()
                .map_err(|_| native_unavailable())?;
            if status.success() {
                Ok(())
            } else {
                Err(native_unavailable())
            }
        }

        fn wake_display(&self) -> Result<(), BleLockError> {
            unsafe {
                let name = cf_string("OneSpace BLE Lock Wake");
                if name.is_null() {
                    return Err(native_unavailable());
                }
                let mut assertion_id: u32 = 0;
                let result = IOPMAssertionDeclareUserActivity(name, 0, &mut assertion_id);
                CFRelease(name);
                if result == 0 {
                    Ok(())
                } else {
                    Err(native_unavailable())
                }
            }
        }

        fn inject_password(&self, password: &str) -> Result<(), BleLockError> {
            inject_password(password)
        }

        fn media_playing(&self) -> Result<bool, BleLockError> {
            media_playing()
        }

        fn media_pause(&self) -> Result<(), BleLockError> {
            media_command(1)
        }

        fn media_play(&self) -> Result<(), BleLockError> {
            media_command(0)
        }
    }

    /// Explicitly request Accessibility trust (asynchronous system prompt).
    pub fn request_accessibility() -> bool {
        request_accessibility_trust()
    }
}

/// Non-macOS platform: every native capability reports a stable unavailable
/// error and no OS action is ever attempted (REQ-002/AC-002).
#[cfg(not(target_os = "macos"))]
mod mac {
    use super::{BleLockKeychainItem, BleLockPlatform};
    use crate::ble_lock::BleLockError;

    /// Unavailable platform used on Windows/Linux and non-Tauri previews.
    #[derive(Clone, Copy, Debug)]
    pub struct MacPlatform;

    fn unavailable() -> BleLockError {
        BleLockError::platform_unavailable()
    }

    impl BleLockPlatform for MacPlatform {
        fn current_user_account(&self) -> String {
            std::env::var("USER")
                .ok()
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "unknown".to_string())
        }

        fn keychain_read(
            &self,
            _item: &BleLockKeychainItem,
        ) -> Result<Option<String>, BleLockError> {
            Err(unavailable())
        }

        fn keychain_write(
            &self,
            _item: &BleLockKeychainItem,
            _password: &str,
        ) -> Result<(), BleLockError> {
            Err(unavailable())
        }

        fn keychain_delete(&self, _item: &BleLockKeychainItem) -> Result<(), BleLockError> {
            Err(unavailable())
        }

        fn accessibility_trusted(&self) -> bool {
            false
        }

        fn is_session_locked(&self) -> bool {
            false
        }

        fn lock_session(&self) -> Result<(), BleLockError> {
            Err(unavailable())
        }

        fn launch_screensaver(&self) -> Result<(), BleLockError> {
            Err(unavailable())
        }

        fn sleep_display(&self) -> Result<(), BleLockError> {
            Err(unavailable())
        }

        fn wake_display(&self) -> Result<(), BleLockError> {
            Err(unavailable())
        }

        fn inject_password(&self, _password: &str) -> Result<(), BleLockError> {
            Err(unavailable())
        }

        fn media_playing(&self) -> Result<bool, BleLockError> {
            Err(BleLockError::new("media_control_unavailable", None))
        }

        fn media_pause(&self) -> Result<(), BleLockError> {
            Err(BleLockError::new("media_control_unavailable", None))
        }

        fn media_play(&self) -> Result<(), BleLockError> {
            Err(BleLockError::new("media_control_unavailable", None))
        }
    }

    /// Non-macOS request is a no-op boolean.
    pub fn request_accessibility() -> bool {
        false
    }
}

pub use mac::MacPlatform;

// ---------------------------------------------------------------------------
// Process-owned runtime, IPC commands and lifecycle (frozen plan Step 2)
// ---------------------------------------------------------------------------

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter};

use crate::ble_lock::{
    BleBluetoothAuthorization, BleBluetoothState, BleCentral, BleCentralEvent, BleDevice,
    BleLockConfigStore, BleLockEffect, BleLockEvent, BleLockMode, BleLockMonitor, BleLockStatus,
    BlePresence,
};

/// Event carrying the one shared [`BleLockStatus`] snapshot (tool + tray).
pub const BLE_LOCK_STATUS_EVENT: &str = "ble-lock-status-updated";
/// Event carrying the discoverable device snapshots.
pub const BLE_LOCK_DEVICES_EVENT: &str = "ble-lock-devices-updated";

/// RSSI/away-lost cadence for the process-owned runtime, independent of UI.
const BLE_LOCK_TICK_INTERVAL: Duration = Duration::from_secs(1);
/// Minimum spacing between reconnect attempts after an unexpected disconnect.
const RECONNECT_THROTTLE_MS: u64 = 5_000;

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

/// Everything the process-owned runtime shares across threads (all `Send`).
struct RuntimeState {
    app: AppHandle,
    config: BleLockConfig,
    monitor: BleLockMonitor,
    controller: BleLockNativeController<MacPlatform>,
    devices: Vec<BleDevice>,
    /// User-requested discovery scan, distinct from monitored-resource desire.
    discovering: bool,
    last_presence: BlePresence,
    /// Last published snapshot so unchanged status is not re-emitted each tick.
    last_status: Option<BleLockStatus>,
    /// Throttle for reconnect attempts after an unexpected disconnect.
    last_reconnect_ms: u64,
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
fn hardware_plan(rt: &RuntimeState) -> (Option<(String, Option<String>)>, BleLockMode, bool, bool) {
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn runtime_guard() -> Option<MutexGuard<'static, RuntimeState>> {
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
        last_reconnect_ms: 0,
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

/// Stop the runtime and release the bridge. Idempotent (REQ-011/AC-014).
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
        guard.controller.note_system_wake();
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

fn current_status() -> Option<BleLockStatus> {
    runtime_guard().map(|guard| build_status(&guard))
}

fn fallback_status() -> BleLockStatus {
    BleLockMonitor::new(BleLockConfig::default()).status()
}

/// Publish the shared snapshot, but only when it actually changed (so async or
/// manual unlock and Accessibility changes are surfaced without per-tick spam).
fn publish_status(app: &AppHandle) {
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
    let _ = app.emit(BLE_LOCK_STATUS_EVENT, status);
}

/// Set a runtime error and force the next status publish to carry it.
fn set_runtime_error(error: BleLockError) {
    if let Some(mut guard) = runtime_guard() {
        guard.monitor.set_last_error(error);
        guard.last_status = None;
    }
}

fn emit_devices(app: &AppHandle) {
    if let Some(guard) = runtime_guard() {
        let _ = app.emit(BLE_LOCK_DEVICES_EVENT, guard.devices.clone());
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

fn apply_reading(rt: &mut RuntimeState, event: BleLockEvent, now_ms: u64) {
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

/// Translate a bridge event into monitor/controller/status updates.
fn handle_central_event(app: &AppHandle, event: BleCentralEvent) {
    let now = now_ms();
    let mut devices_changed = false;
    let mut reapply = false;
    let mut reconnect = false;
    if let Some(mut guard) = runtime_guard() {
        // Ignore any bridge event while asleep or stopped: the resource is
        // released and a rebuild clears stale eligibility on wake.
        if guard.controller.snapshot().stopped || guard.controller.is_system_sleeping() {
            return;
        }
        match event {
            BleCentralEvent::State(state) => {
                guard.monitor.set_bluetooth_state(state);
                match state {
                    BleBluetoothState::PoweredOff => {
                        guard
                            .monitor
                            .set_last_error(BleLockError::bluetooth_powered_off());
                        guard.monitor.set_scanning(false);
                    }
                    BleBluetoothState::Unauthorized => {
                        guard
                            .monitor
                            .set_last_error(BleLockError::bluetooth_unauthorized());
                    }
                    BleBluetoothState::PoweredOn => {
                        // Recovery: drop the stale power/auth error and re-apply
                        // the desired hardware plan now that the radio is ready.
                        let stale = guard
                            .monitor
                            .status()
                            .last_error
                            .as_ref()
                            .map(|error| {
                                matches!(
                                    error.code.as_str(),
                                    "bluetooth_powered_off" | "bluetooth_unauthorized"
                                )
                            })
                            .unwrap_or(false);
                        if stale {
                            guard.monitor.clear_last_error();
                        }
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
                    guard
                        .monitor
                        .set_last_error(BleLockError::bluetooth_unauthorized());
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
                guard.last_status = None;
            }
            BleCentralEvent::ConnectionRead { device_id, rssi } => {
                apply_reading(
                    &mut guard,
                    BleLockEvent::ConnectionRead { device_id, rssi },
                    now,
                );
                guard.last_status = None;
            }
            BleCentralEvent::Connected { .. } => {
                guard.monitor.clear_last_error();
                guard.last_status = None;
            }
            BleCentralEvent::Disconnected { .. } => {
                guard
                    .monitor
                    .set_last_error(BleLockError::device_unavailable());
                guard.last_status = None;
                let powered =
                    guard.monitor.status().bluetooth_state == BleBluetoothState::PoweredOn;
                if desired_monitoring(&guard)
                    && powered
                    && now.saturating_sub(guard.last_reconnect_ms) >= RECONNECT_THROTTLE_MS
                {
                    guard.last_reconnect_ms = now;
                    reconnect = true;
                }
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
    if reapply || reconnect {
        let plan = runtime_guard().map(|guard| hardware_plan(&guard));
        if let Some((selection, mode, scan, background)) = plan {
            dispatch_sync_central(app, selection, mode, scan, background, false);
        }
    }
    publish_status(app);
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
            if effect == BleLockEffect::Lock {
                guard.controller.lock_for_presence();
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
        guard.last_status = None;
        if desired_monitoring(&guard) && guard.config.mode == BleLockMode::Active {
            request_active_rssi = true;
        }
    }
    if request_active_rssi {
        dispatch_request_rssi(app);
    }
    publish_status(app);
    running
}

// ---------------------------------------------------------------------------
// Main-thread CoreBluetooth dispatch
// ---------------------------------------------------------------------------

fn dispatch_request_rssi(app: &AppHandle) {
    let _ = app.run_on_main_thread(|| {
        MAIN_CENTRAL.with(|cell| {
            if let Some(central) = cell.borrow().as_ref() {
                central.request_rssi();
            }
        });
    });
}

/// Release the main-thread bridge (stop + cancel + drop) without creating one.
fn dispatch_release_central(app: &AppHandle) {
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
fn dispatch_sync_central(
    app: &AppHandle,
    selection: Option<(String, Option<String>)>,
    mode: BleLockMode,
    scan: bool,
    background: bool,
    rebuild: bool,
) {
    let handler_app = app.clone();
    let _ = app.run_on_main_thread(move || {
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
                central.set_mode(mode);
                if central.state() == BleBluetoothState::PoweredOn {
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

fn mark_screen_locked(rt: &mut RuntimeState) {
    let locked = rt.controller.snapshot().screen_locked;
    rt.monitor.set_screen_locked(locked);
}

/// Validate and persist a configuration *before* any runtime mutation, so a
/// rejected save leaves the last valid config and running resources untouched;
/// then apply it to the runtime and schedule the resulting hardware plan.
///
/// The controller keeps its manual-lock guard when the selected device id is
/// unchanged (REQ-007); a different id resets eligibility inside `set_config`.
fn commit_config(config: BleLockConfig) -> Result<AppHandle, BleLockError> {
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

// ---------------------------------------------------------------------------
// IPC commands (frozen names; all return the one shared status snapshot)
// ---------------------------------------------------------------------------

/// Return the shared status snapshot (platform availability, radio/presence
/// state, selection, config and last safe error).
#[tauri::command]
pub fn ble_lock_get_status() -> Result<BleLockStatus, BleLockError> {
    Ok(current_status().unwrap_or_else(fallback_status))
}

/// Validate, persist and apply a new configuration.
///
/// Screensaver lock and display-off are mutually exclusive; the caller's copy is
/// normalized (screensaver wins) before persistence so both can never be true.
#[tauri::command]
pub fn ble_lock_save_config(config: BleLockConfig) -> Result<BleLockStatus, BleLockError> {
    let mut config = config;
    // Screensaver lock and display-off are mutually exclusive; screensaver wins.
    if config.use_screensaver_to_lock && config.sleep_display {
        config.sleep_display = false;
    }
    let app = commit_config(config)?;
    publish_status(&app);
    Ok(current_status().unwrap_or_else(fallback_status))
}

/// Start a user discovery scan without halting configured monitoring.
#[tauri::command]
pub fn ble_lock_scan_start() -> Result<BleLockStatus, BleLockError> {
    let (app, selection, mode, scan, background) = {
        let mut guard = match runtime_guard() {
            Some(guard) => guard,
            None => return Ok(fallback_status()),
        };
        guard.discovering = true;
        guard.monitor.set_scanning(true);
        guard.devices.clear();
        guard.last_status = None;
        let plan = hardware_plan(&guard);
        (guard.app.clone(), plan.0, plan.1, plan.2, plan.3)
    };
    dispatch_sync_central(&app, selection, mode, scan, background, false);
    emit_devices(&app);
    publish_status(&app);
    Ok(current_status().unwrap_or_else(fallback_status))
}

/// Stop user discovery; passive monitoring keeps its own scan if configured.
#[tauri::command]
pub fn ble_lock_scan_stop() -> Result<BleLockStatus, BleLockError> {
    let (app, selection, mode, scan, background) = {
        let mut guard = match runtime_guard() {
            Some(guard) => guard,
            None => return Ok(fallback_status()),
        };
        guard.discovering = false;
        guard.monitor.set_scanning(false);
        guard.last_status = None;
        let plan = hardware_plan(&guard);
        (guard.app.clone(), plan.0, plan.1, plan.2, plan.3)
    };
    dispatch_sync_central(&app, selection, mode, scan, background, false);
    publish_status(&app);
    Ok(current_status().unwrap_or_else(fallback_status))
}

/// Select the single monitored device, persisting before any runtime mutation.
#[tauri::command]
pub fn ble_lock_select_device(
    device_id: String,
    device_name: Option<String>,
) -> Result<BleLockStatus, BleLockError> {
    if device_id.trim().is_empty() {
        return Err(BleLockError::device_not_selected());
    }
    let mut config = {
        let guard = match runtime_guard() {
            Some(guard) => guard,
            None => return Ok(fallback_status()),
        };
        guard.config.clone()
    };
    config.device_id = Some(device_id);
    config.device_name = device_name;
    let app = commit_config(config)?;
    publish_status(&app);
    Ok(current_status().unwrap_or_else(fallback_status))
}

/// Clear the selection, cancel connection/scan and persist the cleared state.
#[tauri::command]
pub fn ble_lock_clear_device() -> Result<BleLockStatus, BleLockError> {
    {
        if let Some(mut guard) = runtime_guard() {
            guard.discovering = false;
            guard.devices.clear();
        }
    }
    let mut config = {
        let guard = match runtime_guard() {
            Some(guard) => guard,
            None => return Ok(fallback_status()),
        };
        guard.config.clone()
    };
    config.device_id = None;
    config.device_name = None;
    let app = commit_config(config)?;
    emit_devices(&app);
    publish_status(&app);
    Ok(current_status().unwrap_or_else(fallback_status))
}

/// Store the login password in the profile-local Keychain item (never returned).
#[tauri::command]
pub fn ble_lock_set_password(password: String) -> Result<BleLockStatus, BleLockError> {
    let app = {
        let mut guard = match runtime_guard() {
            Some(guard) => guard,
            None => return Ok(fallback_status()),
        };
        guard.controller.set_password(&password)?;
        guard.monitor.set_password_configured(true);
        guard.last_status = None;
        guard.app.clone()
    };
    publish_status(&app);
    Ok(current_status().unwrap_or_else(fallback_status))
}

/// Delete the profile-local Keychain item.
#[tauri::command]
pub fn ble_lock_delete_password() -> Result<BleLockStatus, BleLockError> {
    let app = {
        let mut guard = match runtime_guard() {
            Some(guard) => guard,
            None => return Ok(fallback_status()),
        };
        guard.controller.delete_password()?;
        guard.monitor.set_password_configured(false);
        guard.last_status = None;
        guard.app.clone()
    };
    publish_status(&app);
    Ok(current_status().unwrap_or_else(fallback_status))
}

/// Explicitly request Accessibility trust (asynchronous system prompt).
#[tauri::command]
pub fn ble_lock_request_accessibility() -> Result<BleLockStatus, BleLockError> {
    let app = {
        let mut guard = match runtime_guard() {
            Some(guard) => guard,
            None => return Ok(fallback_status()),
        };
        let trusted = mac::request_accessibility();
        let observed = guard.controller.platform().accessibility_trusted() || trusted;
        guard.monitor.set_accessibility_trusted(observed);
        guard.last_status = None;
        guard.app.clone()
    };
    publish_status(&app);
    Ok(current_status().unwrap_or_else(fallback_status))
}

/// Manual lock: independent of proximity, Bluetooth and Keychain.
#[tauri::command]
pub fn ble_lock_lock_now() -> Result<BleLockStatus, BleLockError> {
    let app = {
        let mut guard = match runtime_guard() {
            Some(guard) => guard,
            None => return Ok(fallback_status()),
        };
        guard.controller.lock_now();
        mark_screen_locked(&mut guard);
        guard.last_status = None;
        guard.app.clone()
    };
    publish_status(&app);
    Ok(current_status().unwrap_or_else(fallback_status))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    use crate::ble_lock::{BleLockConfig, BleLockError, BlePresence};
    // Resolve the controller types from this file's own module boundary so the
    // single registration path stays the `#[path]` include in
    // `app_runtime/run_app.rs` (`pub(crate) mod ble_lock_native;`). The native
    // Rust module path is not frozen and no `ble_lock.rs` change is required.
    use super::{
        BleLockKeychainItem, BleLockKeychainProfile, BleLockNativeController, BleLockNativeOutcome,
        BleLockPlatform,
    };

    // -----------------------------------------------------------------------
    // Independent fake OS seam
    // -----------------------------------------------------------------------

    /// Every OS-level command the controller is observed to issue.
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum OsCommand {
        LockSession,
        LaunchScreensaver,
        SleepDisplay,
        WakeDisplay,
        InjectPassword(String),
        MediaPause,
        MediaPlay,
        KeychainWrite {
            service: String,
            account: String,
            synchronizable: bool,
        },
        KeychainDelete {
            service: String,
            account: String,
        },
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum MediaState {
        Playing,
        NotPlaying,
        Unavailable,
    }

    #[derive(Clone, Debug)]
    struct FakeOs {
        session_locked: bool,
        display_asleep: bool,
        accessibility_trusted: bool,
        keychain_available: bool,
        keychain: HashMap<(String, String), String>,
        media: MediaState,
        /// Whether an injected password is accepted (the real login password).
        inject_succeeds: bool,
        /// Whether a lock/screensaver request is observed to actually lock.
        lock_observes_locked: bool,
        account: String,
        commands: Vec<OsCommand>,
    }

    /// Shared-handle fake: the controller owns a clone, the test keeps a clone to
    /// configure the observed OS state and inspect the captured command log.
    #[derive(Clone)]
    struct FakePlatform {
        os: Rc<RefCell<FakeOs>>,
    }

    impl FakePlatform {
        fn new() -> Self {
            Self {
                os: Rc::new(RefCell::new(FakeOs {
                    session_locked: false,
                    display_asleep: false,
                    accessibility_trusted: true,
                    keychain_available: true,
                    keychain: HashMap::new(),
                    media: MediaState::NotPlaying,
                    inject_succeeds: true,
                    lock_observes_locked: true,
                    account: "testuser".to_string(),
                    commands: Vec::new(),
                })),
            }
        }

        fn configure(&self, change: impl FnOnce(&mut FakeOs)) {
            change(&mut self.os.borrow_mut());
        }

        fn commands(&self) -> Vec<OsCommand> {
            self.os.borrow().commands.clone()
        }

        fn command_count(&self, predicate: impl Fn(&OsCommand) -> bool) -> usize {
            self.os.borrow().commands.iter().filter(|c| predicate(c)).count()
        }
    }

    impl BleLockPlatform for FakePlatform {
        fn current_user_account(&self) -> String {
            self.os.borrow().account.clone()
        }

        fn keychain_read(&self, item: &BleLockKeychainItem) -> Result<Option<String>, BleLockError> {
            let os = self.os.borrow();
            if !os.keychain_available {
                return Err(BleLockError::new("keychain_unavailable", None));
            }
            Ok(os
                .keychain
                .get(&(item.service.clone(), item.account.clone()))
                .cloned())
        }

        fn keychain_write(
            &self,
            item: &BleLockKeychainItem,
            password: &str,
        ) -> Result<(), BleLockError> {
            let mut os = self.os.borrow_mut();
            if !os.keychain_available {
                return Err(BleLockError::new("keychain_unavailable", None));
            }
            os.commands.push(OsCommand::KeychainWrite {
                service: item.service.clone(),
                account: item.account.clone(),
                synchronizable: item.synchronizable,
            });
            os.keychain
                .insert((item.service.clone(), item.account.clone()), password.to_string());
            Ok(())
        }

        fn keychain_delete(&self, item: &BleLockKeychainItem) -> Result<(), BleLockError> {
            let mut os = self.os.borrow_mut();
            if !os.keychain_available {
                return Err(BleLockError::new("keychain_unavailable", None));
            }
            os.commands.push(OsCommand::KeychainDelete {
                service: item.service.clone(),
                account: item.account.clone(),
            });
            os.keychain
                .remove(&(item.service.clone(), item.account.clone()));
            Ok(())
        }

        fn accessibility_trusted(&self) -> bool {
            self.os.borrow().accessibility_trusted
        }

        // Contract: "observed CURRENT ORDINARY user session locked". Production
        // must return false at loginwindow / pre-boot / any non-current session,
        // and the controller treats false as ineligible for automatic unlock.
        fn is_session_locked(&self) -> bool {
            self.os.borrow().session_locked
        }

        fn lock_session(&self) -> Result<(), BleLockError> {
            let mut os = self.os.borrow_mut();
            os.commands.push(OsCommand::LockSession);
            if os.lock_observes_locked {
                os.session_locked = true;
            }
            Ok(())
        }

        fn launch_screensaver(&self) -> Result<(), BleLockError> {
            let mut os = self.os.borrow_mut();
            os.commands.push(OsCommand::LaunchScreensaver);
            if os.lock_observes_locked {
                os.session_locked = true;
            }
            Ok(())
        }

        fn sleep_display(&self) -> Result<(), BleLockError> {
            let mut os = self.os.borrow_mut();
            os.commands.push(OsCommand::SleepDisplay);
            os.display_asleep = true;
            Ok(())
        }

        fn wake_display(&self) -> Result<(), BleLockError> {
            let mut os = self.os.borrow_mut();
            os.commands.push(OsCommand::WakeDisplay);
            os.display_asleep = false;
            Ok(())
        }

        fn inject_password(&self, password: &str) -> Result<(), BleLockError> {
            let mut os = self.os.borrow_mut();
            os.commands.push(OsCommand::InjectPassword(password.to_string()));
            if os.inject_succeeds {
                os.session_locked = false;
            }
            Ok(())
        }

        fn media_playing(&self) -> Result<bool, BleLockError> {
            match self.os.borrow().media {
                MediaState::Playing => Ok(true),
                MediaState::NotPlaying => Ok(false),
                MediaState::Unavailable => Err(BleLockError::new("media_control_unavailable", None)),
            }
        }

        fn media_pause(&self) -> Result<(), BleLockError> {
            self.os.borrow_mut().commands.push(OsCommand::MediaPause);
            Ok(())
        }

        fn media_play(&self) -> Result<(), BleLockError> {
            self.os.borrow_mut().commands.push(OsCommand::MediaPlay);
            Ok(())
        }
    }

    // -----------------------------------------------------------------------
    // Fixtures / helpers
    // -----------------------------------------------------------------------

    /// Configuration that enables automatic unlock and proximity wake so the
    /// proximity path is fully exercisable.
    fn unlock_ready_config() -> BleLockConfig {
        BleLockConfig {
            unlock_rssi: Some(-50),
            lock_rssi: Some(-70),
            wake_on_proximity: true,
            ..BleLockConfig::default()
        }
    }

    fn injection_count(fake: &FakePlatform) -> usize {
        fake.command_count(|c| matches!(c, OsCommand::InjectPassword(_)))
    }

    fn media_pause_count(fake: &FakePlatform) -> usize {
        fake.command_count(|c| matches!(c, OsCommand::MediaPause))
    }

    fn media_play_count(fake: &FakePlatform) -> usize {
        fake.command_count(|c| matches!(c, OsCommand::MediaPlay))
    }

    fn error_code(outcome: &BleLockNativeOutcome) -> Option<&str> {
        outcome.last_error.as_ref().map(|error| error.code.as_str())
    }

    fn make_controller(
        config: BleLockConfig,
        profile: BleLockKeychainProfile,
        fake: &FakePlatform,
    ) -> BleLockNativeController<FakePlatform> {
        BleLockNativeController::new(config, profile, fake.clone())
    }

    // -----------------------------------------------------------------------
    // REQ-006 / AC-008: password is present/absent only, Keychain profile
    // separation, current-user non-synchronizable generic password
    // -----------------------------------------------------------------------

    #[test]
    fn password_state_is_a_boolean_and_the_secret_never_serializes() {
        let fake = FakePlatform::new();
        let mut controller = make_controller(
            BleLockConfig::default(),
            BleLockKeychainProfile::Release,
            &fake,
        );

        assert!(
            !controller.password_configured(),
            "a fresh controller reports no configured password"
        );

        controller
            .set_password("s3cr3t-login-value")
            .expect("the password is stored in the Keychain stem");
        assert!(
            controller.password_configured(),
            "storing the password flips only the configured flag"
        );

        let snapshot = controller.snapshot();
        let json = serde_json::to_string(&snapshot).expect("snapshot serializes");
        assert!(
            !json.contains("s3cr3t-login-value"),
            "the serializable snapshot must never expose the password"
        );

        controller
            .delete_password()
            .expect("delete the Keychain password");
        assert!(
            !controller.password_configured(),
            "deleting the password clears the configured flag"
        );
    }

    #[test]
    fn release_and_debug_keychain_services_cannot_cross_read() {
        let fake = FakePlatform::new();
        let mut release = make_controller(
            BleLockConfig::default(),
            BleLockKeychainProfile::Release,
            &fake,
        );
        let mut debug = make_controller(
            BleLockConfig::default(),
            BleLockKeychainProfile::Debug,
            &fake,
        );

        release.set_password("release-secret").expect("release write");
        debug.set_password("debug-secret").expect("debug write");

        let services: Vec<String> = fake
            .commands()
            .into_iter()
            .filter_map(|command| match command {
                OsCommand::KeychainWrite { service, .. } => Some(service),
                _ => None,
            })
            .collect();
        assert!(
            services.contains(&"com.onespace.desktop.ble-lock".to_string()),
            "the Release profile must use the frozen Release service name"
        );
        assert!(
            services.contains(&"com.onespace.desktop.ble-lock.dev".to_string()),
            "the Debug profile must use the frozen Debug service name"
        );

        // Isolation: a profile only observes its own generic-password item.
        let isolated = FakePlatform::new();
        let mut release = make_controller(
            BleLockConfig::default(),
            BleLockKeychainProfile::Release,
            &isolated,
        );
        let debug = make_controller(
            BleLockConfig::default(),
            BleLockKeychainProfile::Debug,
            &isolated,
        );
        release.set_password("only-release").expect("release write");
        assert!(release.password_configured());
        assert!(
            !debug.password_configured(),
            "the Debug service must not read the Release item"
        );
    }

    #[test]
    fn keychain_item_targets_the_current_user_and_is_not_synchronized() {
        let fake = FakePlatform::new();
        let controller = make_controller(
            BleLockConfig::default(),
            BleLockKeychainProfile::Release,
            &fake,
        );

        let item = controller.keychain_item();
        assert_eq!(
            item.account,
            fake.current_user_account(),
            "the Keychain account must be the current user"
        );
        assert!(
            !item.synchronizable,
            "the login password must never be synchronizable"
        );

        let mut controller = controller;
        controller.set_password("pw").expect("write the password");
        let writes: Vec<OsCommand> = fake
            .commands()
            .into_iter()
            .filter(|command| matches!(command, OsCommand::KeychainWrite { .. }))
            .collect();
        assert_eq!(writes.len(), 1, "exactly one Keychain write is observed");
        match &writes[0] {
            OsCommand::KeychainWrite {
                account,
                synchronizable,
                ..
            } => {
                assert_eq!(account, &fake.current_user_account());
                assert!(!*synchronizable);
            }
            other => panic!("expected a Keychain write, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // REQ-006 / AC-009: one guarded unlock attempt
    // -----------------------------------------------------------------------

    #[test]
    fn near_after_away_injects_the_password_exactly_once() {
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.session_locked = true;
            os.accessibility_trusted = true;
        });
        let mut controller = make_controller(unlock_ready_config(), BleLockKeychainProfile::Release, &fake);
        controller
            .set_password("login-pw")
            .expect("configure a password");

        controller.note_away_or_lost();
        let outcome = controller.note_near();

        assert!(
            outcome.password_injected,
            "a fully qualified away/lost -> near transition attempts one injection"
        );
        assert_eq!(injection_count(&fake), 1);
        assert!(
            fake.commands()
                .contains(&OsCommand::InjectPassword("login-pw".to_string())),
            "the configured password is what reaches the OS seam"
        );

        // Further near readings in the same episode must not retry.
        controller.note_near();
        controller.note_near();
        assert_eq!(
            injection_count(&fake),
            1,
            "at most one injection attempt per qualifying transition"
        );
    }

    #[test]
    fn missing_gates_prevent_injection() {
        // (a) Keychain unavailable.
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.session_locked = true;
            os.accessibility_trusted = true;
        });
        let mut controller = make_controller(unlock_ready_config(), BleLockKeychainProfile::Release, &fake);
        controller.set_password("pw").expect("configure");
        fake.configure(|os| os.keychain_available = false);
        controller.note_away_or_lost();
        let outcome = controller.note_near();
        assert!(!outcome.password_injected);
        assert_eq!(injection_count(&fake), 0);
        assert_eq!(error_code(&outcome), Some("keychain_unavailable"));

        // (b) Accessibility not trusted.
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.session_locked = true;
            os.accessibility_trusted = false;
        });
        let mut controller = make_controller(unlock_ready_config(), BleLockKeychainProfile::Release, &fake);
        controller.set_password("pw").expect("configure");
        controller.note_away_or_lost();
        let outcome = controller.note_near();
        assert!(!outcome.password_injected);
        assert_eq!(injection_count(&fake), 0);

        // (c) No current ordinary locked session (already unlocked).
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.session_locked = false;
            os.accessibility_trusted = true;
        });
        let mut controller = make_controller(unlock_ready_config(), BleLockKeychainProfile::Release, &fake);
        controller.set_password("pw").expect("configure");
        controller.note_away_or_lost();
        let outcome = controller.note_near();
        assert!(!outcome.password_injected);
        assert_eq!(injection_count(&fake), 0);

        // (d) Automatic unlock disabled.
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.session_locked = true;
            os.accessibility_trusted = true;
        });
        let mut config = unlock_ready_config();
        config.unlock_rssi = None;
        let mut controller = make_controller(config, BleLockKeychainProfile::Release, &fake);
        controller.set_password("pw").expect("configure");
        controller.note_away_or_lost();
        let outcome = controller.note_near();
        assert!(!outcome.password_injected);
        assert_eq!(injection_count(&fake), 0);

        // (e) No password configured.
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.session_locked = true;
            os.accessibility_trusted = true;
        });
        let mut controller = make_controller(unlock_ready_config(), BleLockKeychainProfile::Release, &fake);
        controller.note_away_or_lost();
        let outcome = controller.note_near();
        assert!(!outcome.password_injected);
        assert_eq!(injection_count(&fake), 0);
    }

    #[test]
    fn a_wrong_password_still_locked_is_never_retried_in_the_same_episode() {
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.session_locked = true;
            os.inject_succeeds = false;
            os.accessibility_trusted = true;
        });
        let mut controller = make_controller(unlock_ready_config(), BleLockKeychainProfile::Release, &fake);
        controller.set_password("wrong-pw").expect("configure");

        controller.note_away_or_lost();
        let first = controller.note_near();
        assert!(first.password_injected);
        assert!(
            fake.is_session_locked(),
            "the wrong password leaves the session observed locked"
        );

        for _ in 0..5 {
            controller.note_near();
        }
        assert_eq!(
            injection_count(&fake),
            1,
            "an observed locked session must not create a retry loop"
        );

        // A genuinely new away/lost -> near transition restores eligibility.
        controller.note_away_or_lost();
        controller.note_near();
        assert_eq!(injection_count(&fake), 2);
    }

    #[test]
    fn wake_without_unlocking_wakes_the_display_but_never_injects() {
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.session_locked = true;
            os.display_asleep = true;
            os.accessibility_trusted = true;
        });
        let mut config = unlock_ready_config();
        config.wake_without_unlocking = true;
        let mut controller = make_controller(config, BleLockKeychainProfile::Release, &fake);
        controller.set_password("pw").expect("configure");

        controller.note_away_or_lost();
        let outcome = controller.note_near();

        assert!(outcome.display_woke, "the display wake is preserved");
        assert!(
            !outcome.password_injected,
            "Wake without unlocking must suppress injection"
        );
        assert_eq!(injection_count(&fake), 0);
        assert!(fake.commands().contains(&OsCommand::WakeDisplay));
    }

    // -----------------------------------------------------------------------
    // REQ-007 / AC-010: manual-lock priority
    // -----------------------------------------------------------------------

    #[test]
    fn lock_now_is_independent_of_proximity() {
        let fake = FakePlatform::new();
        let mut controller = make_controller(BleLockConfig::default(), BleLockKeychainProfile::Release, &fake);

        // No device selection or presence has ever been observed.
        let outcome = controller.lock_now();

        assert!(outcome.lock_requested, "Lock Now never waits for RSSI");
        assert!(
            fake.commands().contains(&OsCommand::LockSession),
            "Lock Now requests an ordinary lock"
        );
    }

    #[test]
    fn a_manual_lock_while_still_near_cannot_unlock_until_away_then_near() {
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.session_locked = false;
            os.accessibility_trusted = true;
            os.lock_observes_locked = true;
        });
        let mut controller = make_controller(unlock_ready_config(), BleLockKeychainProfile::Release, &fake);
        controller.set_password("pw").expect("configure");

        let outcome = controller.lock_now();
        assert!(outcome.lock_requested);
        assert!(outcome.locked);
        assert!(fake.is_session_locked());

        // Still near after the manual lock: no proximity unlock may fire.
        let near = controller.note_near();
        assert!(!near.password_injected);
        assert_eq!(injection_count(&fake), 0);

        // Only a later away/lost followed by near restores eligibility.
        controller.note_away_or_lost();
        let resumed = controller.note_near();
        assert!(resumed.password_injected);
        assert_eq!(injection_count(&fake), 1);
    }

    // -----------------------------------------------------------------------
    // REQ-008 / AC-011: wake display, system sleep suppression, wake rebuild
    // -----------------------------------------------------------------------

    #[test]
    fn system_sleep_suppresses_proximity_and_wake_rebuilds_intent() {
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.session_locked = true;
            os.display_asleep = true;
            os.accessibility_trusted = true;
        });
        let mut controller = make_controller(unlock_ready_config(), BleLockKeychainProfile::Release, &fake);
        controller.set_password("pw").expect("configure");

        controller.note_away_or_lost();
        controller.note_system_sleep();
        let during_sleep = controller.note_near();
        assert!(
            !during_sleep.display_woke,
            "system sleep pauses the proximity display decision"
        );
        assert!(!during_sleep.password_injected);
        assert_eq!(injection_count(&fake), 0);
        assert!(!fake.commands().contains(&OsCommand::WakeDisplay));

        controller.note_system_wake();
        // Wake rebuilds intent: a near without a fresh away/lost must not unlock.
        let after_wake = controller.note_near();
        assert!(
            !after_wake.password_injected,
            "wake resets the unlock intent until a new away/lost -> near transition"
        );

        controller.note_away_or_lost();
        let resumed = controller.note_near();
        assert!(resumed.password_injected);
        assert_eq!(injection_count(&fake), 1);
    }

    // -----------------------------------------------------------------------
    // REQ-009 / AC-012: media pause/resume
    // -----------------------------------------------------------------------

    #[test]
    fn lock_pauses_only_confirmed_playing_media_with_an_explicit_pause() {
        // Confirmed playing -> one explicit pause, never a toggle.
        let fake = FakePlatform::new();
        fake.configure(|os| os.media = MediaState::Playing);
        let config = BleLockConfig {
            pause_now_playing: true,
            ..BleLockConfig::default()
        };
        let mut controller = make_controller(config.clone(), BleLockKeychainProfile::Release, &fake);
        let outcome = controller.lock_now();
        assert!(outcome.media_paused);
        assert_eq!(media_pause_count(&fake), 1);
        assert_eq!(
            media_play_count(&fake),
            0,
            "a lock must not toggle media playback"
        );

        // Not playing -> no pause at all.
        let fake = FakePlatform::new();
        fake.configure(|os| os.media = MediaState::NotPlaying);
        let mut controller = make_controller(config, BleLockKeychainProfile::Release, &fake);
        let outcome = controller.lock_now();
        assert!(!outcome.media_paused);
        assert_eq!(media_pause_count(&fake), 0);
    }

    #[test]
    fn unavailable_media_reports_the_warning_but_still_locks() {
        let fake = FakePlatform::new();
        fake.configure(|os| os.media = MediaState::Unavailable);
        let config = BleLockConfig {
            pause_now_playing: true,
            ..BleLockConfig::default()
        };
        let mut controller = make_controller(config, BleLockKeychainProfile::Release, &fake);

        let outcome = controller.lock_now();

        assert!(outcome.lock_requested, "media failure must not block locking");
        assert_eq!(error_code(&outcome), Some("media_control_unavailable"));
        assert!(!outcome.media_paused, "no pause may be claimed");
        assert_eq!(media_pause_count(&fake), 0);
    }

    #[test]
    fn unlock_resumes_only_the_media_paused_in_the_current_cycle_exactly_once() {
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.media = MediaState::Playing;
            os.session_locked = false;
            os.inject_succeeds = true;
            os.accessibility_trusted = true;
        });
        let mut config = unlock_ready_config();
        config.pause_now_playing = true;
        let mut controller = make_controller(config, BleLockKeychainProfile::Release, &fake);
        controller.set_password("pw").expect("configure");

        controller.lock_now();
        assert_eq!(media_pause_count(&fake), 1, "the lock cycle pauses media");

        controller.note_away_or_lost();
        let outcome = controller.note_near();
        assert!(outcome.password_injected);
        assert!(outcome.media_resumed);
        assert_eq!(
            media_play_count(&fake),
            1,
            "the recorded current-cycle pause resumes exactly once after an observed unlock"
        );

        // A later transition must not replay the already-consumed pause.
        controller.note_away_or_lost();
        controller.note_near();
        assert_eq!(media_play_count(&fake), 1);
    }

    #[test]
    fn unlock_without_a_recorded_cycle_pause_does_not_play() {
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.media = MediaState::NotPlaying;
            os.session_locked = true;
            os.inject_succeeds = true;
            os.accessibility_trusted = true;
        });
        let mut config = unlock_ready_config();
        config.pause_now_playing = true;
        let mut controller = make_controller(config, BleLockKeychainProfile::Release, &fake);
        controller.set_password("pw").expect("configure");

        // No lock cycle ever paused anything.
        controller.note_away_or_lost();
        let outcome = controller.note_near();

        assert!(outcome.password_injected);
        assert!(!outcome.media_resumed);
        assert_eq!(media_play_count(&fake), 0);
    }

    // -----------------------------------------------------------------------
    // REQ-010 / AC-013: screensaver, lock and display-off outcomes
    // -----------------------------------------------------------------------

    #[test]
    fn a_screensaver_lock_succeeds_only_when_the_session_is_observed_locked() {
        let screensaver_config = BleLockConfig {
            use_screensaver_to_lock: true,
            ..BleLockConfig::default()
        };

        // Observed locked -> success.
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.lock_observes_locked = true;
            os.session_locked = false;
        });
        let mut controller = make_controller(screensaver_config.clone(), BleLockKeychainProfile::Release, &fake);
        let outcome = controller.lock_now();
        assert!(outcome.lock_requested);
        assert!(outcome.locked);
        assert!(fake.commands().contains(&OsCommand::LaunchScreensaver));

        // Screensaver launched but the session is not observed locked -> no claim.
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.lock_observes_locked = false;
            os.session_locked = false;
        });
        let mut controller = make_controller(screensaver_config, BleLockKeychainProfile::Release, &fake);
        let outcome = controller.lock_now();
        assert!(
            !outcome.locked,
            "an unobserved screensaver lock must never be reported as success"
        );
        assert!(fake.commands().contains(&OsCommand::LaunchScreensaver));
    }

    #[test]
    fn screensaver_mode_never_turns_off_the_display() {
        let fake = FakePlatform::new();
        fake.configure(|os| os.lock_observes_locked = true);
        let config = BleLockConfig {
            use_screensaver_to_lock: true,
            sleep_display: true,
            ..BleLockConfig::default()
        };
        let mut controller = make_controller(config, BleLockKeychainProfile::Release, &fake);

        controller.lock_now();

        assert!(fake.commands().contains(&OsCommand::LaunchScreensaver));
        assert_eq!(
            fake.command_count(|c| matches!(c, OsCommand::SleepDisplay)),
            0,
            "screensaver lock and display-off are mutually exclusive"
        );
    }

    #[test]
    fn an_ordinary_lock_sleeps_the_display_only_after_an_observed_lock() {
        let display_config = BleLockConfig {
            sleep_display: true,
            ..BleLockConfig::default()
        };

        // Observed lock -> display off strictly after the lock.
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.lock_observes_locked = true;
            os.session_locked = false;
        });
        let mut controller = make_controller(display_config.clone(), BleLockKeychainProfile::Release, &fake);
        controller.lock_now();
        let commands = fake.commands();
        let lock_index = commands
            .iter()
            .position(|c| matches!(c, OsCommand::LockSession))
            .expect("an ordinary lock is requested");
        let sleep_index = commands
            .iter()
            .position(|c| matches!(c, OsCommand::SleepDisplay))
            .expect("the display is turned off after the lock");
        assert!(
            lock_index < sleep_index,
            "ordinary display-off must follow the lock"
        );

        // Unobserved lock -> no display off.
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.lock_observes_locked = false;
            os.session_locked = false;
        });
        let mut controller = make_controller(display_config, BleLockKeychainProfile::Release, &fake);
        let outcome = controller.lock_now();
        assert!(!outcome.locked);
        assert!(!outcome.display_slept);
        assert_eq!(
            fake.command_count(|c| matches!(c, OsCommand::SleepDisplay)),
            0,
            "display-off must not happen without an observed lock"
        );
    }

    // -----------------------------------------------------------------------
    // REQ-011 / AC-014: resource lifecycle and idempotent cleanup
    // -----------------------------------------------------------------------

    #[test]
    fn shutdown_is_idempotent_and_stops_further_native_actions() {
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.session_locked = true;
            os.display_asleep = true;
            os.accessibility_trusted = true;
        });
        let mut controller = make_controller(unlock_ready_config(), BleLockKeychainProfile::Release, &fake);
        controller.set_password("pw").expect("configure");

        controller.shutdown();
        controller.shutdown(); // repeated cleanup must be safe
        assert!(controller.snapshot().stopped, "shutdown marks the runtime stopped");

        let before = fake.commands().len();
        controller.note_away_or_lost();
        let near = controller.note_near();
        let lock = controller.lock_now();
        assert!(!near.password_injected);
        assert!(!lock.lock_requested);
        assert_eq!(
            fake.commands().len(),
            before,
            "stopped resources must issue no further native actions"
        );
    }

    // -----------------------------------------------------------------------
    // REQ-013 / AC-017: stable, non-secret error taxonomy
    // -----------------------------------------------------------------------

    #[test]
    fn keychain_errors_use_a_stable_code_and_never_echo_the_password() {
        let secret = "S3cr3t-P@ssw0rd";
        let fake = FakePlatform::new();
        fake.configure(|os| os.keychain_available = false);
        let mut controller = make_controller(BleLockConfig::default(), BleLockKeychainProfile::Release, &fake);

        let error = controller
            .set_password(secret)
            .expect_err("a Keychain failure must surface");

        assert_eq!(error.code, "keychain_unavailable");
        assert!(
            !format!("{error:?}").contains(secret),
            "an error must never echo the supplied password"
        );
        if let Some(detail) = &error.detail {
            assert!(!detail.contains(secret));
        }
    }

    #[test]
    fn native_outcomes_serialize_errors_without_a_password() {
        let secret = "another-top-secret";
        let fake = FakePlatform::new();
        fake.configure(|os| os.media = MediaState::Unavailable);
        let config = BleLockConfig {
            pause_now_playing: true,
            ..BleLockConfig::default()
        };
        let mut controller = make_controller(config, BleLockKeychainProfile::Release, &fake);
        controller.set_password(secret).expect("configure a password");

        let outcome = controller.lock_now();
        let json = serde_json::to_string(&outcome).expect("outcome serializes");

        assert_eq!(error_code(&outcome), Some("media_control_unavailable"));
        assert!(
            !json.contains(secret),
            "a serialized native outcome must never carry the password"
        );
    }

    // -----------------------------------------------------------------------
    // Production composition: presence adapter, config replacement, sleep gate,
    // async observed-unlock polling and AC-017 gate feedback
    // -----------------------------------------------------------------------

    /// Active selected configuration whose lock/unlock thresholds are -70/-50,
    /// so an averaged RSSI of -65 sits between them and -45 clears unlock.
    fn selected_active_config() -> BleLockConfig {
        let mut config = unlock_ready_config();
        config.device_id = Some("uuid-a".to_string());
        config
    }

    #[test]
    fn observe_presence_injects_only_at_the_unlock_threshold_and_only_once() {
        // The production adapter must translate observed presence + averaged RSSI
        // into native policy. In production, `update_presence` is called while the
        // monitor presence stays `Near`, so a plain `note_near` on the first near
        // reading would inject before the unlock threshold and consume the only
        // attempt.
        let fake = FakePlatform::new();
        fake.configure(|os| os.session_locked = true);
        let mut controller = make_controller(
            selected_active_config(),
            BleLockKeychainProfile::Release,
            &fake,
        );
        controller.set_password("login-pw").expect("configure a password");

        // Away at -85 arms exactly one eligibility.
        let away = controller.observe_presence(BlePresence::Away, Some(-85));
        assert!(!away.password_injected);
        assert_eq!(injection_count(&fake), 0);

        // Near but between lock (-70) and unlock (-50): no injection yet.
        let between = controller.observe_presence(BlePresence::Near, Some(-65));
        assert!(
            !between.password_injected,
            "an RSSI below the unlock threshold must not inject"
        );
        assert_eq!(injection_count(&fake), 0);

        // Near at/above unlock (-50): exactly one injection.
        let eligible = controller.observe_presence(BlePresence::Near, Some(-45));
        assert!(
            eligible.password_injected,
            "an RSSI at/above unlock injects once"
        );
        assert_eq!(injection_count(&fake), 1);

        // A repeated near reading must not inject again.
        let repeated = controller.observe_presence(BlePresence::Near, Some(-45));
        assert!(!repeated.password_injected);
        assert_eq!(
            injection_count(&fake),
            1,
            "a repeated near reading must not re-inject"
        );
    }

    #[test]
    fn replacing_the_selected_device_clears_old_eligibility() {
        let fake = FakePlatform::new();
        fake.configure(|os| os.session_locked = true);
        let mut controller = make_controller(
            selected_active_config(),
            BleLockKeychainProfile::Release,
            &fake,
        );
        controller.set_password("pw").expect("configure");

        // Arm for the old device while it is still selected.
        controller.note_away_or_lost();

        // Selecting a different UUID must clear the old arming/cycle state.
        let mut replacement = selected_active_config();
        replacement.device_id = Some("uuid-b".to_string());
        controller.set_config(replacement);

        let stale = controller.note_near();
        assert!(
            !stale.password_injected,
            "a device replacement must clear the previous arming"
        );
        assert_eq!(injection_count(&fake), 0);

        // A fresh away/lost under the new device restores exactly one attempt.
        controller.note_away_or_lost();
        let fresh = controller.note_near();
        assert!(fresh.password_injected);
        assert_eq!(injection_count(&fake), 1);
    }

    #[test]
    fn a_presence_lock_is_suppressed_during_system_sleep_and_resumes_after_wake() {
        let fake = FakePlatform::new();
        fake.configure(|os| os.lock_observes_locked = true);
        let mut controller = make_controller(
            selected_active_config(),
            BleLockKeychainProfile::Release,
            &fake,
        );

        controller.note_system_sleep();
        let during_sleep = controller.lock_for_presence();
        assert!(
            !during_sleep.lock_requested,
            "an observed presence lock must be suppressed while the system sleeps"
        );
        assert!(!during_sleep.locked);
        assert_eq!(
            fake.commands().len(),
            0,
            "no native lock action may run during system sleep"
        );

        controller.note_system_wake();
        let after_wake = controller.lock_for_presence();
        assert!(
            after_wake.lock_requested,
            "a presence lock resumes after system wake"
        );
        assert!(fake.commands().contains(&OsCommand::LockSession));
    }

    #[test]
    fn poll_native_state_resumes_recorded_media_once_on_an_observed_async_unlock() {
        // A user/manual unlock is observed asynchronously (no password injection),
        // so the recorded current-cycle pause must be resumed by polling the
        // observed native state, not only by the synchronous injection path.
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.media = MediaState::Playing;
            os.session_locked = false;
            os.lock_observes_locked = true;
        });
        let config = BleLockConfig {
            pause_now_playing: true,
            ..BleLockConfig::default()
        };
        let mut controller = make_controller(config, BleLockKeychainProfile::Release, &fake);

        controller.lock_now();
        assert_eq!(media_pause_count(&fake), 1, "the lock cycle pauses media");

        // Poll while the session is still locked: nothing resumes.
        let locked_poll = controller.poll_native_state();
        assert!(!locked_poll.media_resumed);
        assert_eq!(media_play_count(&fake), 0);

        // The user unlocks asynchronously; the next poll resumes exactly once.
        fake.configure(|os| os.session_locked = false);
        let resumed = controller.poll_native_state();
        assert!(
            resumed.media_resumed,
            "an observed async unlock must resume the recorded pause"
        );
        assert_eq!(media_play_count(&fake), 1);

        // Further polls must not replay the consumed resume.
        let again = controller.poll_native_state();
        assert!(!again.media_resumed);
        assert_eq!(media_play_count(&fake), 1);
    }

    #[test]
    fn unlock_gate_feedback_reports_accessibility_and_missing_password() {
        // Missing Accessibility trust: no injection and stable feedback.
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.session_locked = true;
            os.accessibility_trusted = false;
        });
        let mut controller = make_controller(
            selected_active_config(),
            BleLockKeychainProfile::Release,
            &fake,
        );
        controller.set_password("pw").expect("configure a password");
        controller.note_away_or_lost();
        let untrusted = controller.note_near();
        assert!(!untrusted.password_injected);
        assert_eq!(injection_count(&fake), 0);
        assert_eq!(
            error_code(&untrusted),
            Some("accessibility_required"),
            "an untrusted Accessibility gate must surface actionable feedback"
        );

        // No password configured: no injection and stable feedback.
        let fake = FakePlatform::new();
        fake.configure(|os| {
            os.session_locked = true;
            os.accessibility_trusted = true;
        });
        let mut controller = make_controller(
            selected_active_config(),
            BleLockKeychainProfile::Release,
            &fake,
        );
        controller.note_away_or_lost();
        let unconfigured = controller.note_near();
        assert!(!unconfigured.password_injected);
        assert_eq!(injection_count(&fake), 0);
        assert_eq!(
            error_code(&unconfigured),
            Some("password_not_configured"),
            "a missing Keychain password must surface actionable feedback"
        );
    }
}
