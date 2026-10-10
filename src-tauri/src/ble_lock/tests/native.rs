//! Delegated behavior tests for the `ble-lock` native controller (frozen plan
//! `20261010-ble-lock-tool` Step 2).
//!
//! Moved verbatim from the inline `#[cfg(test)] mod tests` at the tail of
//! `src-tauri/src/ble_lock/native.rs` (review finding R5 test layout), plus
//! appended review regressions (R1 safe account FFI, R2 snapshot Keychain fault
//! surfacing, R3 Bluetooth-state radio feedback, R4 stopped-state wake lifecycle).
//! The module is a child of `native`, so `super::` still resolves the native
//! controller's public types; no private runtime state is inspected.
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

    // -----------------------------------------------------------------------
    // R1: safe current-account FFI query (macOS)
    // -----------------------------------------------------------------------

    /// The real account lookup must be a safe public API rather than the old
    /// handwritten `getpwuid` passwd parsing; this asserts the actual user against
    /// a read-only system oracle and never touches Keychain/hardware/session.
    #[cfg(target_os = "macos")]
    #[test]
    fn mac_platform_current_account_name_matches_the_system_oracle() {
        use super::MacPlatform;

        let oracle = std::process::Command::new("/usr/bin/id")
            .arg("-un")
            .output()
            .expect("run the read-only /usr/bin/id -un account oracle");
        assert!(
            oracle.status.success(),
            "/usr/bin/id -un must succeed to serve as the account oracle"
        );
        let expected = String::from_utf8(oracle.stdout)
            .expect("id -un output is valid UTF-8")
            .trim()
            .to_string();
        assert!(!expected.is_empty(), "the account oracle must be non-empty");

        let actual = MacPlatform::current_account_name()
            .expect("the safe account query must succeed for the current user");
        assert_eq!(
            actual.trim(),
            expected,
            "the native account query must equal /usr/bin/id -un"
        );
    }

    // -----------------------------------------------------------------------
    // R2: snapshot surfaces an unreadable Keychain without losing manual lock
    // -----------------------------------------------------------------------

    #[test]
    fn snapshot_surfaces_a_keychain_read_fault_and_keeps_manual_lock() {
        let secret = "snapshot-secret-value";
        let fake = FakePlatform::new();
        fake.configure(|os| os.lock_observes_locked = true);
        let mut controller = make_controller(
            BleLockConfig::default(),
            BleLockKeychainProfile::Release,
            &fake,
        );
        controller.set_password(secret).expect("store the password");

        // The Keychain becomes unreadable: the snapshot must not collapse the
        // fault into a bare `false`.
        fake.configure(|os| os.keychain_available = false);
        let snapshot = controller.snapshot();
        assert!(
            !snapshot.password_configured,
            "an unreadable Keychain is not a configured password"
        );
        assert_eq!(
            snapshot.last_error.as_ref().map(|error| error.code.as_str()),
            Some("keychain_unavailable"),
            "an unreadable Keychain must surface a stable warning"
        );
        let rendered = serde_json::to_string(&snapshot).expect("snapshot serializes");
        assert!(
            !rendered.contains(secret),
            "the snapshot must never contain the password"
        );

        // Independent manual lock stays available while the read fault is active.
        let outcome = controller.lock_now();
        assert!(outcome.lock_requested, "manual Lock Now stays available");
    }

    // -----------------------------------------------------------------------
    // R3: Bluetooth-state radio feedback adapter
    // -----------------------------------------------------------------------

    #[test]
    fn observe_bluetooth_state_reports_unsupported_and_clears_on_powered_on() {
        use crate::ble_lock::BleBluetoothState;

        let fake = FakePlatform::new();
        let mut controller = make_controller(
            BleLockConfig::default(),
            BleLockKeychainProfile::Release,
            &fake,
        );

        controller.observe_bluetooth_state(BleBluetoothState::Unsupported);
        assert_eq!(
            controller
                .snapshot()
                .last_error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("bluetooth_unsupported"),
            "an unsupported radio must surface the frozen stable code"
        );

        controller.observe_bluetooth_state(BleBluetoothState::PoweredOn);
        assert_eq!(
            controller
                .snapshot()
                .last_error
                .as_ref()
                .map(|error| error.code.as_str()),
            None,
            "a powered-on radio clears the previous radio error"
        );
    }

    // -----------------------------------------------------------------------
    // R4: stopped-state wake lifecycle contract
    // -----------------------------------------------------------------------

    #[test]
    fn wake_after_shutdown_reports_no_rebuild_and_stays_stopped() {
        let fake = FakePlatform::new();
        fake.configure(|os| os.lock_observes_locked = true);
        let mut controller = make_controller(
            unlock_ready_config(),
            BleLockKeychainProfile::Release,
            &fake,
        );

        // Alive path: a sleep/wake cycle may rebuild hardware.
        controller.note_system_sleep();
        assert!(
            controller.note_system_wake(),
            "an awake, running controller may rebuild hardware after wake"
        );
        assert!(!controller.snapshot().stopped);

        // Stopped path: wake must refuse to rebuild and stay stopped.
        controller.shutdown();
        assert!(
            !controller.note_system_wake(),
            "a stopped controller must not rebuild hardware after wake"
        );
        assert!(controller.snapshot().stopped);
        controller.shutdown();
        assert!(!controller.note_system_wake());
        assert!(controller.snapshot().stopped);

        assert_eq!(
            fake.commands().len(),
            0,
            "the wake lifecycle must issue no OS command"
        );
    }
