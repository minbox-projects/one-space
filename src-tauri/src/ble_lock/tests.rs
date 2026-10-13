//! Delegated behavior tests for frozen plan Step 1 (`20261010-ble-lock-tool`).
//!
//! These tests exercise only the public `crate::ble_lock` boundary: the
//! validated profile-local configuration seam and the single-device
//! monitoring/presence state machine. They use independent explicit RSSI and
//! clock fixtures and never touch private functions, hardware, Keychain,
//! native OS actions (Step 2) or the frontend/tray (Step 3).
//!
//! The product `ble_lock` module is added by the backend implementation step.
//! Until then this file is compiled through the `#[cfg(test)]` discovery hook in
//! `lib.rs` and fails as a genuine RED baseline for the absent public API.

use std::fs;
use std::path::{Path, PathBuf};

use crate::ble_lock::{
    BleBluetoothState, BleDevice, BleLockConfig, BleLockConfigStore, BleLockEffect, BleLockError,
    BleLockEvent, BleLockMode, BleLockMonitor, BlePresence,
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Self-cleaning temporary directory used as an isolated profile root.
struct TempRoot(PathBuf);

impl TempRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "onespace-ble-lock-{}-{}-{}",
            label,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).expect("create temp root");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn device(id: &str, rssi: i16) -> BleDevice {
    BleDevice {
        id: id.to_string(),
        name: format!("Device {id}"),
        rssi,
    }
}

fn advertisement(id: &str, rssi: i16) -> BleLockEvent {
    BleLockEvent::Advertisement {
        device_id: id.to_string(),
        rssi,
    }
}

fn connection_read(id: &str, rssi: i16) -> BleLockEvent {
    BleLockEvent::ConnectionRead {
        device_id: id.to_string(),
        rssi,
    }
}

fn base_config() -> BleLockConfig {
    BleLockConfig::default()
}

fn count_unlocks(effects: &[BleLockEffect]) -> usize {
    effects
        .iter()
        .filter(|effect| **effect == BleLockEffect::Unlock)
        .count()
}

fn count_locks(effects: &[BleLockEffect]) -> usize {
    effects
        .iter()
        .filter(|effect| **effect == BleLockEffect::Lock)
        .count()
}

fn has_connect(effects: &[BleLockEffect]) -> bool {
    effects
        .iter()
        .any(|effect| *effect == BleLockEffect::Connect)
}

// ---------------------------------------------------------------------------
// REQ-004 / AC-005: defaults, threshold boundaries, delay bounds
// ---------------------------------------------------------------------------

#[test]
fn default_config_is_disabled_and_defaults_to_active_mode() {
    let config = BleLockConfig::default();

    assert_eq!(config.mode, BleLockMode::Active, "active mode is the default");
    assert_eq!(config.unlock_rssi, None, "unlock action starts disabled");
    assert_eq!(config.lock_rssi, None, "lock action starts disabled");
    assert!(!config.wake_on_proximity);
    assert!(!config.wake_without_unlocking);
    assert!(!config.pause_now_playing);
    assert!(!config.use_screensaver_to_lock);
    assert!(!config.sleep_display);
    assert!(
        config.validate().is_ok(),
        "the disabled default configuration must be valid"
    );
}

#[test]
fn thresholds_accept_the_full_range_in_five_dbm_steps() {
    for value in (-95..=-30).step_by(5) {
        let value = value as i16;

        let unlock = BleLockConfig {
            unlock_rssi: Some(value),
            ..base_config()
        };
        assert!(
            unlock.validate().is_ok(),
            "unlock threshold {value} is an accepted boundary value"
        );

        let lock = BleLockConfig {
            lock_rssi: Some(value),
            ..base_config()
        };
        assert!(
            lock.validate().is_ok(),
            "lock threshold {value} is an accepted boundary value"
        );
    }
}

#[test]
fn thresholds_reject_out_of_range_and_non_step_values() {
    for value in [-100i16, -25, -92, -33, -29] {
        let unlock = BleLockConfig {
            unlock_rssi: Some(value),
            ..base_config()
        };
        let error = unlock.validate().expect_err("out-of-contract unlock value");
        assert_eq!(error.code, "invalid_config");

        let lock = BleLockConfig {
            lock_rssi: Some(value),
            ..base_config()
        };
        let error = lock.validate().expect_err("out-of-contract lock value");
        assert_eq!(error.code, "invalid_config");
    }
}

#[test]
fn ordering_rule_applies_only_when_both_thresholds_are_enabled() {
    let ordered = BleLockConfig {
        unlock_rssi: Some(-30),
        lock_rssi: Some(-60),
        ..base_config()
    };
    assert!(
        ordered.validate().is_ok(),
        "unlock above lock must be accepted when both are set"
    );

    let equal = BleLockConfig {
        unlock_rssi: Some(-50),
        lock_rssi: Some(-50),
        ..base_config()
    };
    assert!(
        equal.validate().is_ok(),
        "unlock equal to lock is the stated greater-than-or-equal boundary"
    );

    let inverted = BleLockConfig {
        unlock_rssi: Some(-70),
        lock_rssi: Some(-50),
        ..base_config()
    };
    let error = inverted
        .validate()
        .expect_err("unlock below lock must be rejected when both are set");
    assert_eq!(error.code, "invalid_config");

    // A lone threshold never triggers the ordering rule regardless of its value.
    let unlock_only = BleLockConfig {
        unlock_rssi: Some(-70),
        lock_rssi: None,
        ..base_config()
    };
    assert!(unlock_only.validate().is_ok());

    let lock_only = BleLockConfig {
        unlock_rssi: None,
        lock_rssi: Some(-30),
        ..base_config()
    };
    assert!(lock_only.validate().is_ok());
}

#[test]
fn lock_delay_and_no_signal_timeout_bounds_are_enforced() {
    for (lock_delay_secs, no_signal_timeout_secs, accepted) in [
        (0u32, 5u32, false),
        (1, 5, true),
        (3600, 3600, true),
        (3601, 3600, false),
        (1, 4, false),
        (1, 3601, false),
    ] {
        let config = BleLockConfig {
            lock_delay_secs,
            no_signal_timeout_secs,
            ..base_config()
        };
        assert_eq!(
            config.validate().is_ok(),
            accepted,
            "lock_delay={lock_delay_secs}s timeout={no_signal_timeout_secs}s"
        );
    }
}

// ---------------------------------------------------------------------------
// REQ-004/REQ-012 / AC-005/AC-015: validated profile-local persistence
// ---------------------------------------------------------------------------

#[test]
fn config_store_returns_defaults_when_absent_and_round_trips_a_valid_config() {
    let root = TempRoot::new("round-trip");
    let store = BleLockConfigStore::at(root.path());
    let path = root.path().join("ble_lock.json");

    assert_eq!(
        store.load().expect("load from a missing file is the default"),
        BleLockConfig::default()
    );
    assert!(!path.exists(), "loading must not create ble_lock.json");

    let config = BleLockConfig {
        device_id: Some("uuid-1".to_string()),
        device_name: Some("My Phone".to_string()),
        mode: BleLockMode::Passive,
        unlock_rssi: Some(-50),
        lock_rssi: Some(-80),
        wake_on_proximity: true,
        ..BleLockConfig::default()
    };
    store.save(&config).expect("save a valid config");

    assert!(path.exists(), "saving must create the profile-local ble_lock.json");
    assert_eq!(store.load().expect("load saved config"), config);
}

#[test]
fn invalid_config_save_preserves_previous_bytes_and_state() {
    let root = TempRoot::new("invalid-preserve");
    let store = BleLockConfigStore::at(root.path());
    let path = root.path().join("ble_lock.json");

    let valid = BleLockConfig {
        device_id: Some("uuid-1".to_string()),
        lock_rssi: Some(-60),
        ..BleLockConfig::default()
    };
    store.save(&valid).expect("save the last valid config");
    let before = fs::read(&path).expect("read bytes before the invalid save");

    let invalid = BleLockConfig {
        lock_rssi: Some(-97),
        ..BleLockConfig::default()
    };
    let error = store.save(&invalid).expect_err("invalid config must be rejected");
    assert_eq!(error.code, "invalid_config");

    let after = fs::read(&path).expect("read bytes after the invalid save");
    assert_eq!(
        after, before,
        "a rejected save must not rewrite the previous ble_lock.json bytes"
    );
    assert_eq!(
        store.load().expect("load after rejected save"),
        valid,
        "the last valid configuration must remain observable"
    );
}

#[test]
fn config_store_isolates_release_and_debug_profile_roots() {
    let home = TempRoot::new("profile-home");
    let release_root = crate::config::app_dir_for(home.path(), false);
    let debug_root = crate::config::app_dir_for(home.path(), true);
    assert_ne!(
        release_root, debug_root,
        "release and debug profile roots must be distinct"
    );

    let release_store = BleLockConfigStore::at(&release_root);
    let debug_store = BleLockConfigStore::at(&debug_root);

    let release_config = BleLockConfig {
        device_name: Some("release-device".to_string()),
        ..BleLockConfig::default()
    };
    let debug_config = BleLockConfig {
        device_name: Some("debug-device".to_string()),
        ..BleLockConfig::default()
    };
    release_store.save(&release_config).expect("save release config");
    debug_store.save(&debug_config).expect("save debug config");

    assert_eq!(
        release_store
            .load()
            .expect("load release config")
            .device_name
            .as_deref(),
        Some("release-device")
    );
    assert_eq!(
        debug_store
            .load()
            .expect("load debug config")
            .device_name
            .as_deref(),
        Some("debug-device")
    );
    assert!(release_root.join("ble_lock.json").exists());
    assert!(debug_root.join("ble_lock.json").exists());
}

#[test]
fn serialized_config_has_the_frozen_fields_and_never_a_password() {
    let config = BleLockConfig {
        device_id: Some("uuid-1".to_string()),
        device_name: Some("Phone".to_string()),
        unlock_rssi: Some(-40),
        lock_rssi: Some(-70),
        ..BleLockConfig::default()
    };

    let value = serde_json::to_value(&config).expect("serialize config");
    for field in [
        "schema_version",
        "device_id",
        "device_name",
        "mode",
        "unlock_rssi",
        "lock_rssi",
        "lock_delay_secs",
        "no_signal_timeout_secs",
        "wake_on_proximity",
        "wake_without_unlocking",
        "pause_now_playing",
        "use_screensaver_to_lock",
        "sleep_display",
    ] {
        assert!(
            value.get(field).is_some(),
            "config must serialize the frozen field `{field}`"
        );
    }
    assert_eq!(
        value.get("mode").and_then(|mode| mode.as_str()),
        Some("active"),
        "mode must serialize as the frozen lowercase value"
    );

    let root = TempRoot::new("no-password");
    let store = BleLockConfigStore::at(root.path());
    store.save(&config).expect("save config for inspection");
    let raw = fs::read_to_string(root.path().join("ble_lock.json")).expect("read saved config");
    assert!(
        !value.to_string().to_lowercase().contains("password"),
        "the serializable configuration must not expose a password field"
    );
    assert!(
        !raw.to_lowercase().contains("password"),
        "ble_lock.json must never contain a password field"
    );
}

// ---------------------------------------------------------------------------
// REQ-003 / AC-003/AC-004: discovery modes and single-device selection
// ---------------------------------------------------------------------------

#[test]
fn active_mode_connects_on_selection_and_consumes_connection_reads() {
    let config = BleLockConfig {
        mode: BleLockMode::Active,
        ..base_config()
    };
    let mut monitor = BleLockMonitor::new(config);

    let effects = monitor.select_device(device("uuid-a", -40));
    assert_eq!(
        effects,
        vec![BleLockEffect::Connect],
        "active selection must establish a connection"
    );
    assert_eq!(
        monitor.status().presence,
        BlePresence::Unknown,
        "selection alone must not invent presence"
    );

    let effects = monitor.feed(connection_read("uuid-a", -40), 1_000);
    assert!(effects.is_empty(), "a near reading emits no lock/unlock intent");

    let status = monitor.status();
    assert_eq!(status.presence, BlePresence::Near);
    assert_eq!(status.averaged_rssi, Some(-40));
    assert_eq!(
        status.selected_device.as_ref().map(|d| d.id.as_str()),
        Some("uuid-a")
    );
}

#[test]
fn active_mode_accepts_connection_reads_and_ignores_advertisements() {
    // `BleLockEvent` carries both sources with the same device id and rssi, so
    // the accepted source is mode-dependent: active mode derives RSSI only from
    // connection reads; passive mode only from advertisements (REQ-003/AC-003).
    let config = BleLockConfig {
        mode: BleLockMode::Active,
        ..base_config()
    };
    let mut monitor = BleLockMonitor::new(config);

    let effects = monitor.select_device(device("uuid-active", -42));
    assert_eq!(effects, vec![BleLockEffect::Connect]);

    let effects = monitor.feed(advertisement("uuid-active", -42), 1_000);
    assert!(
        effects.is_empty(),
        "an active-mode advertisement must not emit a lock/unlock intent"
    );
    let status = monitor.status();
    assert_eq!(
        status.presence,
        BlePresence::Unknown,
        "active mode must not derive presence from advertisements"
    );
    assert_eq!(
        status.averaged_rssi, None,
        "an advertisement must not enter the active-mode smoothing window"
    );

    let effects = monitor.feed(connection_read("uuid-active", -37), 2_000);
    assert!(effects.is_empty());
    let status = monitor.status();
    assert_eq!(status.presence, BlePresence::Near);
    assert_eq!(status.averaged_rssi, Some(-37));
}

#[test]
fn passive_mode_uses_advertisements_and_never_connects() {
    let config = BleLockConfig {
        mode: BleLockMode::Passive,
        ..base_config()
    };
    let mut monitor = BleLockMonitor::new(config);

    let effects = monitor.select_device(device("uuid-p", -55));
    assert!(
        effects.is_empty(),
        "passive selection must not emit a connect effect"
    );

    let effects = monitor.feed(advertisement("uuid-p", -55), 1_000);
    assert!(effects.is_empty());
    assert_eq!(monitor.status().averaged_rssi, Some(-55));
    assert_eq!(monitor.status().presence, BlePresence::Near);

    // Without a connection, a connection read is not a valid passive source and
    // must not establish presence.
    let mut fresh = BleLockMonitor::new(BleLockConfig {
        mode: BleLockMode::Passive,
        ..base_config()
    });
    fresh.select_device(device("uuid-p", -55));
    fresh.feed(connection_read("uuid-p", -55), 1_000);
    let status = fresh.status();
    assert_eq!(status.presence, BlePresence::Unknown);
    assert_eq!(status.averaged_rssi, None);
}

#[test]
fn replacing_or_clearing_the_device_rejects_stale_uuid_and_resets_history() {
    let mut monitor = BleLockMonitor::new(base_config());
    monitor.select_device(device("uuid-a", -40));
    monitor.feed(connection_read("uuid-a", -40), 1_000);
    assert_eq!(monitor.status().averaged_rssi, Some(-40));

    let effects = monitor.select_device(device("uuid-b", -55));
    assert_eq!(
        effects,
        vec![BleLockEffect::Connect],
        "active replacement connects to the new device"
    );
    let status = monitor.status();
    assert_eq!(
        status.selected_device.as_ref().map(|d| d.id.as_str()),
        Some("uuid-b")
    );
    assert_eq!(status.averaged_rssi, None, "replacement clears old history");
    assert_eq!(status.presence, BlePresence::Unknown);

    // A reading for the replaced device is stale and must be ignored.
    let stale = monitor.feed(connection_read("uuid-a", -90), 2_000);
    assert!(stale.is_empty());
    let status = monitor.status();
    assert_eq!(
        status.selected_device.as_ref().map(|d| d.id.as_str()),
        Some("uuid-b")
    );
    assert_eq!(status.averaged_rssi, None, "stale readings must not update state");
    assert_eq!(status.presence, BlePresence::Unknown);

    monitor.clear_device();
    let status = monitor.status();
    assert!(status.selected_device.is_none(), "clear removes the selection");
    assert_eq!(status.presence, BlePresence::Unknown);
    assert_eq!(status.averaged_rssi, None);

    // With no selection, no reading or tick may trigger anything.
    assert!(monitor.feed(connection_read("uuid-b", -90), 3_000).is_empty());
    assert!(monitor.tick(600_000).is_empty());
}

// ---------------------------------------------------------------------------
// REQ-005 / AC-006/AC-007: smoothing, debounce, lost timeout, one-shot intents
// ---------------------------------------------------------------------------

#[test]
fn averaged_rssi_is_the_mean_of_at_most_five_recent_readings() {
    let mut monitor = BleLockMonitor::new(base_config());
    monitor.select_device(device("uuid-s", -50));

    for (index, value) in [-50i16, -55, -60, -65, -70].iter().enumerate() {
        monitor.feed(
            connection_read("uuid-s", *value),
            (index as u64 + 1) * 1_000,
        );
    }
    // mean(-50, -55, -60, -65, -70) = -300 / 5 = -60
    assert_eq!(monitor.status().averaged_rssi, Some(-60));

    monitor.feed(connection_read("uuid-s", -30), 6_000);
    // last five: -55, -60, -65, -70, -30 -> -280 / 5 = -56
    assert_eq!(monitor.status().averaged_rssi, Some(-56));
}

#[test]
fn continuous_below_lock_emits_exactly_one_lock_after_the_delay() {
    let config = BleLockConfig {
        lock_rssi: Some(-60),
        lock_delay_secs: 10,
        no_signal_timeout_secs: 30,
        ..base_config()
    };
    let mut monitor = BleLockMonitor::new(config);
    monitor.select_device(device("uuid-l", -90));
    monitor.feed(connection_read("uuid-l", -90), 0);

    assert!(
        monitor.tick(9_000).is_empty(),
        "before the lock delay elapses no lock is emitted"
    );
    assert_eq!(monitor.status().presence, BlePresence::Unknown);

    let effects = monitor.tick(10_000);
    assert_eq!(effects, vec![BleLockEffect::Lock]);
    assert_eq!(monitor.status().presence, BlePresence::Away);

    assert!(
        monitor.tick(20_000).is_empty(),
        "a departure episode must emit only one lock"
    );
    assert_eq!(monitor.status().presence, BlePresence::Away);
}

#[test]
fn recovery_before_the_delay_cancels_the_pending_lock() {
    let config = BleLockConfig {
        lock_rssi: Some(-60),
        lock_delay_secs: 10,
        no_signal_timeout_secs: 60,
        ..base_config()
    };
    let mut monitor = BleLockMonitor::new(config);
    monitor.select_device(device("uuid-r", -90));
    monitor.feed(connection_read("uuid-r", -90), 0);

    // mean(-90, -10) = -50, at or above the -60 lock threshold: a valid recovery.
    monitor.feed(connection_read("uuid-r", -10), 5_000);
    assert_eq!(monitor.status().presence, BlePresence::Near);

    assert!(
        monitor.tick(60_000).is_empty(),
        "a recovered reading must clear the pending lock timer"
    );
    assert_eq!(monitor.status().presence, BlePresence::Near);
}

#[test]
fn absent_readings_emit_one_lock_only_when_lock_is_enabled() {
    let enabled = BleLockConfig {
        lock_rssi: Some(-60),
        lock_delay_secs: 10,
        no_signal_timeout_secs: 5,
        ..base_config()
    };
    let mut monitor = BleLockMonitor::new(enabled);
    monitor.select_device(device("uuid-x", -40));
    monitor.feed(connection_read("uuid-x", -40), 0);

    assert!(monitor.tick(4_000).is_empty(), "before the timeout nothing fires");
    let effects = monitor.tick(5_000);
    assert_eq!(effects, vec![BleLockEffect::Lock]);
    assert_eq!(monitor.status().presence, BlePresence::Lost);
    assert!(
        monitor.tick(60_000).is_empty(),
        "a lost episode must emit only one lock"
    );
    assert_eq!(monitor.status().presence, BlePresence::Lost);

    let disabled = BleLockConfig {
        lock_rssi: None,
        no_signal_timeout_secs: 5,
        ..base_config()
    };
    let mut monitor = BleLockMonitor::new(disabled);
    monitor.select_device(device("uuid-y", -40));
    monitor.feed(connection_read("uuid-y", -40), 0);

    let effects = monitor.tick(5_000);
    assert!(
        effects.is_empty(),
        "signal loss without an enabled lock must not emit a lock"
    );
    assert_eq!(monitor.status().presence, BlePresence::Lost);
}

// ---------------------------------------------------------------------------
// REQ-007 / AC-010: manual-lock priority
// ---------------------------------------------------------------------------

#[test]
fn manual_lock_suppresses_near_unlock_until_away_or_lost_then_near() {
    let config = BleLockConfig {
        unlock_rssi: Some(-50),
        lock_rssi: Some(-60),
        lock_delay_secs: 10,
        no_signal_timeout_secs: 600,
        ..base_config()
    };
    let mut monitor = BleLockMonitor::new(config);
    monitor.select_device(device("uuid-m", -30));
    monitor.feed(connection_read("uuid-m", -30), 0);
    assert_eq!(monitor.status().presence, BlePresence::Near);

    let effects = monitor.manual_lock(1_000);
    assert_eq!(
        effects,
        vec![BleLockEffect::Lock],
        "Lock Now locks without waiting for RSSI"
    );

    // Still near after a manual lock: no proximity unlock may fire.
    assert!(
        monitor.feed(connection_read("uuid-m", -30), 2_000).is_empty(),
        "a near device must not re-unlock immediately after a manual lock"
    );

    // Drive the mean below the lock threshold and let the departure delay elapse.
    for now_ms in [3_000u64, 4_000, 5_000, 6_000] {
        monitor.feed(connection_read("uuid-m", -90), now_ms);
    }
    let away = monitor.tick(16_000);
    assert_eq!(away, vec![BleLockEffect::Lock]);
    assert_eq!(monitor.status().presence, BlePresence::Away);

    // A new near transition after away restores exactly one unlock attempt.
    let mut unlocks = 0;
    for now_ms in [17_000u64, 18_000, 19_000, 20_000, 21_000, 22_000] {
        let effects = monitor.feed(connection_read("uuid-m", -30), now_ms);
        unlocks += count_unlocks(&effects);
    }
    assert_eq!(
        unlocks, 1,
        "away then near must restore a single unlock eligibility"
    );
    assert_eq!(monitor.status().presence, BlePresence::Near);
}

// ---------------------------------------------------------------------------
// REQ-002 / AC-002: platform availability is exposed on the status snapshot
// ---------------------------------------------------------------------------

#[test]
fn status_reports_platform_availability_for_the_build_target() {
    let monitor = BleLockMonitor::new(base_config());
    assert_eq!(
        monitor.status().platform_available,
        cfg!(target_os = "macos"),
        "platform_available must reflect the compiled target"
    );
}

// ---------------------------------------------------------------------------
// BLE-return regression: a transport failure after "device left" must retry the
// connection on the periodic tick, throttled, until a valid reading returns.
// ---------------------------------------------------------------------------

#[test]
fn device_unavailable_retries_connect_periodically_until_recovery() {
    // Active monitoring with lock and unlock enabled, so the selected device
    // stays eligible for automatic (re)connection and later proximity actions.
    let config = BleLockConfig {
        mode: BleLockMode::Active,
        lock_rssi: Some(-60),
        unlock_rssi: Some(-50),
        ..base_config()
    };
    let mut monitor = BleLockMonitor::new(config);
    monitor.set_bluetooth_state(BleBluetoothState::PoweredOn);

    let initial = monitor.select_device(device("uuid-return", -40));
    assert_eq!(
        initial,
        vec![BleLockEffect::Connect],
        "selecting an active device establishes the initial connection"
    );

    // A healthy near reading: the device is present and its signal is not lost.
    let reading = monitor.feed(connection_read("uuid-return", -40), 1_000);
    assert!(
        reading.is_empty(),
        "a near reading emits no lock/unlock intent"
    );
    let status = monitor.status();
    assert_eq!(status.presence, BlePresence::Near);
    assert_eq!(status.averaged_rssi, Some(-40));
    assert_eq!(
        status.selected_device.as_ref().map(|d| d.id.as_str()),
        Some("uuid-return")
    );

    // The transport then reports the selected device is unreachable: the
    // "device left" state that must recover without a manual reselect.
    monitor.set_last_error(BleLockError::device_unavailable());

    // The periodic tick itself must retry the connection; it cannot wait for a
    // CoreBluetooth disconnect callback that may never arrive inside the window.
    let first_retry = monitor.tick(10_000);
    assert!(
        has_connect(&first_retry),
        "a tick after device_unavailable must emit an automatic Connect retry"
    );

    // A quick failure inside the five-second window must not busy-loop.
    let within_window = monitor.tick(11_000);
    assert!(
        !has_connect(&within_window),
        "a tick inside the 5s retry window must not emit another Connect"
    );

    // ... nor may it stop retrying: a later tick past the window retries again
    // with no new scan and no re-selection.
    let later_retry = monitor.tick(15_000);
    assert!(
        has_connect(&later_retry),
        "a tick at or beyond 5s must retry Connect without a new scan or reselect"
    );

    // Retrying preserves the user's selection and monitored presence.
    let status = monitor.status();
    assert_eq!(
        status.selected_device.as_ref().map(|d| d.id.as_str()),
        Some("uuid-return"),
        "automatic retries must preserve the selected UUID"
    );
    assert!(status.monitoring, "the selected device stays monitored");
    assert_eq!(status.presence, BlePresence::Near, "signal was never lost");

    // The outage outlasts the no-signal timeout: the selected device is now
    // genuinely LOST (last valid reading at 1s, default 30s timeout). The tick
    // must classify the loss, emit exactly one Lock for the episode, and keep
    // retrying Connect without dropping the selection.
    let lost = monitor.tick(31_000);
    assert_eq!(
        monitor.status().presence,
        BlePresence::Lost,
        "no valid reading within the 30s timeout must classify the device as lost"
    );
    assert_eq!(
        count_locks(&lost),
        1,
        "the lost episode must emit exactly one Lock"
    );
    assert!(
        has_connect(&lost),
        "a lost-but-unavailable device must still emit an automatic Connect retry"
    );
    assert_eq!(
        monitor
            .status()
            .selected_device
            .as_ref()
            .map(|d| d.id.as_str()),
        Some("uuid-return"),
        "a long outage that reaches lost must preserve the selected UUID"
    );

    // A quick follow-up tick inside the retry window must neither busy-loop nor
    // repeat the Lock for the same lost episode.
    let followup = monitor.tick(32_000);
    assert!(
        !has_connect(&followup),
        "a tick inside the 5s window must not emit another Connect while lost"
    );
    assert_eq!(
        count_locks(&followup),
        0,
        "a single lost episode must emit only one Lock"
    );
    assert_eq!(
        monitor.status().presence,
        BlePresence::Lost,
        "a throttled tick must not change the lost classification"
    );

    // A later tick past the window keeps retrying Connect, still with no repeat
    // Lock and no new scan or re-selection.
    let resumed = monitor.tick(36_000);
    assert!(
        has_connect(&resumed),
        "a tick at or beyond 5s must keep retrying Connect while still unavailable"
    );
    assert_eq!(
        count_locks(&resumed),
        0,
        "continuing retries must not repeat the Lock for the same episode"
    );

    // The device returns after the long outage with a valid active connection
    // reading: it resolves device_unavailable, restores near, and spends the one
    // unlock eligibility that the lost episode armed.
    let recovered = monitor.feed(connection_read("uuid-return", -40), 40_000);
    assert_eq!(
        count_unlocks(&recovered),
        1,
        "lost-then-near must restore exactly one unlock eligibility"
    );
    let status = monitor.status();
    assert_eq!(
        status.last_error, None,
        "a valid selected active reading must clear device_unavailable"
    );
    assert_eq!(
        status.presence,
        BlePresence::Near,
        "recovery after lost restores the near state"
    );
    assert_eq!(status.averaged_rssi, Some(-40));
    assert_eq!(
        status.selected_device.as_ref().map(|d| d.id.as_str()),
        Some("uuid-return"),
        "recovery must preserve the selected device"
    );

    // A second identical reading must not unlock again.
    let repeated = monitor.feed(connection_read("uuid-return", -40), 41_000);
    assert_eq!(
        count_unlocks(&repeated),
        0,
        "only one unlock is eligible per lost-then-near episode"
    );

    // After recovery the transport is no longer unavailable, so the periodic
    // tick must not emit any further automatic Connect retry.
    let after_recovery = monitor.tick(45_000);
    assert!(
        !has_connect(&after_recovery),
        "a recovered device must not receive an automatic Connect retry"
    );
    assert_eq!(
        count_locks(&after_recovery),
        0,
        "a recovered near device must not emit a Lock"
    );

    // Only the transient unavailability is resolved: an unrelated error stays.
    monitor.set_last_error(BleLockError::platform_unavailable());
    monitor.feed(connection_read("uuid-return", -40), 46_000);
    assert_eq!(
        monitor.status().last_error,
        Some(BleLockError::platform_unavailable()),
        "recovery must preserve unrelated errors"
    );
}
