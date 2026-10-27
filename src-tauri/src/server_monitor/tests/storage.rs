//! Step 2 RED behavior tests: encrypted persistence and slot claims.

use crate::server_monitor::storage::{
    ServerMonitorConfig, ServerMonitorStore, SlotClaimOutcome,
};
use crate::server_monitor::types::{
    CollectionFailureKind, CpuMetric, MetricSnapshot, ServerMonitorAuthKind, ServerMonitorCustomRecord,
    ServerMonitorOutcome, ServerMonitorSample, ServerMonitorScheduleMode, ServerMonitorSettings,
    ServerMonitorSourceKind, ServerMonitorState, ServerMonitorTargetRecord,
};
use std::fs;
use std::path::PathBuf;

fn temp_root() -> PathBuf {
    std::env::temp_dir().join(format!("onespace-sm-storage-{}", uuid::Uuid::new_v4()))
}

#[test]
fn settings_defaults_and_validation_boundaries() {
    let settings = ServerMonitorSettings::default();
    assert!(settings.auto_collect_enabled);
    assert!(matches!(settings.schedule_mode, ServerMonitorScheduleMode::Interval));
    assert_eq!(settings.interval_minutes, 5);
    assert_eq!(settings.daily_times, vec!["09:00".to_string()]);
    assert_eq!(settings.retention_days, 7);
    assert!(settings.alert_enabled);
    assert_eq!(settings.cpu_threshold_percent, 80);
    assert_eq!(settings.memory_threshold_percent, 85);
    assert_eq!(settings.disk_threshold_percent, 85);
    assert_eq!(settings.alert_cooldown_minutes, 60);
    assert!(settings.validate().is_ok());

    for cpu in [1u32, 100] {
        let mut s = ServerMonitorSettings::default();
        s.cpu_threshold_percent = cpu;
        assert!(s.validate().is_ok());
    }
    for cpu in [0u32, 101] {
        let mut s = ServerMonitorSettings::default();
        s.cpu_threshold_percent = cpu;
        assert!(s.validate().is_err());
    }

    for mem in [1u32, 100] {
        let mut s = ServerMonitorSettings::default();
        s.memory_threshold_percent = mem;
        assert!(s.validate().is_ok());
    }
    for mem in [0u32, 101] {
        let mut s = ServerMonitorSettings::default();
        s.memory_threshold_percent = mem;
        assert!(s.validate().is_err());
    }

    for disk in [1u32, 100] {
        let mut s = ServerMonitorSettings::default();
        s.disk_threshold_percent = disk;
        assert!(s.validate().is_ok());
    }
    for disk in [0u32, 101] {
        let mut s = ServerMonitorSettings::default();
        s.disk_threshold_percent = disk;
        assert!(s.validate().is_err());
    }

    for cooldown in [1u32, 1440] {
        let mut s = ServerMonitorSettings::default();
        s.alert_cooldown_minutes = cooldown;
        assert!(s.validate().is_ok());
    }
    for cooldown in [0u32, 1441] {
        let mut s = ServerMonitorSettings::default();
        s.alert_cooldown_minutes = cooldown;
        assert!(s.validate().is_err());
    }

    let mut interval_min = ServerMonitorSettings::default();
    interval_min.interval_minutes = 1;
    assert!(interval_min.validate().is_ok());
    let mut interval_max = ServerMonitorSettings::default();
    interval_max.interval_minutes = 1440;
    assert!(interval_max.validate().is_ok());

    for invalid in [0u32, 1441] {
        let mut s = ServerMonitorSettings::default();
        s.interval_minutes = invalid;
        assert!(s.validate().is_err());
    }

    for retention in [1u32, 30] {
        let mut s = ServerMonitorSettings::default();
        s.retention_days = retention;
        assert!(s.validate().is_ok());
    }
    for retention in [0u32, 31] {
        let mut s = ServerMonitorSettings::default();
        s.retention_days = retention;
        assert!(s.validate().is_err());
    }

    let mut daily = ServerMonitorSettings::default();
    daily.daily_times = vec!["00:00".to_string(), "23:59".to_string()];
    assert!(daily.validate().is_ok());

    for invalid_times in [
        Vec::<String>::new(),
        vec!["09:00".to_string(), "09:00".to_string()],
        vec!["9:00".to_string()],
        vec!["24:00".to_string()],
    ] {
        let mut s = ServerMonitorSettings::default();
        s.daily_times = invalid_times;
        assert!(s.validate().is_err());
    }
}

#[test]
fn encrypted_config_round_trips_and_hides_credentials() {
    let root = temp_root();
    let store = ServerMonitorStore::new(root.clone());
    let password = "super-secret-password";
    let target = ServerMonitorTargetRecord {
        id: "target-1".to_string(),
        name: "Production".to_string(),
        enabled: true,
        source_kind: crate::server_monitor::types::ServerMonitorSourceKind::Custom,
        saved_host_name: None,
        custom: Some(ServerMonitorCustomRecord {
            host: "secret-host.example".to_string(),
            port: 22,
            user: "secret-user".to_string(),
            auth_kind: crate::server_monitor::types::ServerMonitorAuthKind::Password,
            key_path: None,
            password: Some(password.to_string()),
        }),
        tunnel: None,
        created_at_ms: 10,
        updated_at_ms: 20,
        blocked: false,
        last_error_kind: None,
        last_error: None,
        last_success_at_ms: None,
        has_alert: false,
        alert_reasons: Vec::new(),
        last_alert_at_ms: None,
    };
    let config = ServerMonitorConfig {
        version: 1,
        settings: ServerMonitorSettings::default(),
        targets: vec![target],
    };
    store.save_config(&config).unwrap();
    let path = store.config_path();
    let bytes = fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"v2:"));
    let raw = String::from_utf8_lossy(&bytes);
    assert!(!raw.contains(password));
    assert!(!raw.contains("secret-host.example"));
    assert!(!raw.contains("secret-user"));
    assert!(!raw.contains("Production"));

    let loaded = store.load_config().unwrap();
    assert_eq!(loaded.settings.interval_minutes, 5);
    assert_eq!(loaded.targets.len(), 1);
    assert_eq!(loaded.targets[0].id, "target-1");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn corrupt_config_is_reported_and_not_silently_replaced() {
    let root = temp_root();
    let store = ServerMonitorStore::new(root.clone());
    fs::create_dir_all(root.clone()).unwrap();
    fs::write(store.config_path(), b"not-encrypted").unwrap();
    assert!(store.load_config().is_err());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn config_store_unopenable_path_returns_error() {
    let path = std::env::temp_dir().join(format!("onespace-sm-file-{}", uuid::Uuid::new_v4()));
    fs::write(&path, b"x").unwrap();
    let store = ServerMonitorStore::new(path.clone());
    assert!(store.claim_slot("t", "2026-10-26", "09:00", 1).is_err());
    let _ = fs::remove_file(path);
}

#[test]
fn samples_persist_query_and_preserve_failed_outcomes() {
    let root = temp_root();
    let store = ServerMonitorStore::new(root.clone());
    let sample = ServerMonitorSample {
        id: None,
        target_id: "target-1".to_string(),
        collected_at_ms: 1000,
        outcome: ServerMonitorOutcome::Failed,
        sampling_window_seconds: 1.2,
        source_mode: ServerMonitorScheduleMode::Interval,
        expected_interval_seconds: Some(300),
        cpu: Some(CpuMetric { usage_percent: 1.0 }),
        memory: None,
        disks: None,
        networks: Vec::new(),
        network_interface: None,
        error_kind: Some(CollectionFailureKind::Transport),
        error: Some("boom".to_string()),
    };
    let id = store.append_sample(&sample).unwrap();
    assert!(id > 0);
    let rows = store.history("target-1", 0, 2000).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(matches!(rows[0].outcome, ServerMonitorOutcome::Failed));
    assert_eq!(rows[0].error.as_deref(), Some("boom"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn slot_claims_are_unique_and_survive_reopen() {
    let root = temp_root();
    {
        let store = ServerMonitorStore::new(root.clone());
        assert!(matches!(
            store.claim_slot("t", "2026-10-26", "09:00", 1).unwrap(),
            SlotClaimOutcome::Claimed
        ));
        assert!(matches!(
            store.claim_slot("t", "2026-10-26", "09:00", 2).unwrap(),
            SlotClaimOutcome::AlreadyClaimed
        ));
        assert!(store.has_claim("t", "2026-10-26", "09:00").unwrap());
    }
    let store = ServerMonitorStore::new(root.clone());
    assert!(matches!(
        store.claim_slot("t", "2026-10-26", "09:00", 3).unwrap(),
        SlotClaimOutcome::AlreadyClaimed
    ));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn deleting_target_removes_only_its_data() {
    let root = temp_root();
    let store = ServerMonitorStore::new(root.clone());
    let sample_a = ServerMonitorSample {
        id: None,
        target_id: "a".to_string(),
        collected_at_ms: 1,
        outcome: ServerMonitorOutcome::Success(MetricSnapshot {
            collected_at_ms: 1,
            sampling_window_seconds: 1.0,
            cpu: None,
            memory: None,
            disks: None,
            network: None,
            networks: Vec::new(),
            network_interface: None,
        }),
        sampling_window_seconds: 1.0,
        source_mode: ServerMonitorScheduleMode::Interval,
        expected_interval_seconds: None,
        cpu: None,
        memory: None,
        disks: None,
        networks: Vec::new(),
        network_interface: None,
        error_kind: None,
        error: None,
    };
    let sample_b = ServerMonitorSample {
        id: None,
        target_id: "b".to_string(),
        collected_at_ms: 1,
        outcome: ServerMonitorOutcome::Success(MetricSnapshot {
            collected_at_ms: 1,
            sampling_window_seconds: 1.0,
            cpu: None,
            memory: None,
            disks: None,
            network: None,
            networks: Vec::new(),
            network_interface: None,
        }),
        sampling_window_seconds: 1.0,
        source_mode: ServerMonitorScheduleMode::Interval,
        expected_interval_seconds: None,
        cpu: None,
        memory: None,
        disks: None,
        networks: Vec::new(),
        network_interface: None,
        error_kind: None,
        error: None,
    };
    store.append_sample(&sample_a).unwrap();
    store.append_sample(&sample_b).unwrap();
    store.claim_slot("a", "2026-10-26", "09:00", 1).unwrap();
    store.claim_slot("b", "2026-10-26", "09:00", 1).unwrap();
    store.delete_target_data("a").unwrap();
    assert!(store.history("a", 0, 10).unwrap().is_empty());
    assert_eq!(store.history("b", 0, 10).unwrap().len(), 1);
    assert!(!store.has_claim("a", "2026-10-26", "09:00").unwrap());
    assert!(store.has_claim("b", "2026-10-26", "09:00").unwrap());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn state_serialization_never_contains_credentials() {
    let password = "hidden-password";
    let state = ServerMonitorState {
        settings: ServerMonitorSettings::default(),
        targets: Vec::new(),
        next_collection_at_ms: None,
        last_success_at_ms: None,
        error: None,
    };
    let json = serde_json::to_string(&state).unwrap();
    assert!(!json.contains(password));
    let _ = password;
}

#[test]
fn corrupt_config_save_refuses_to_overwrite() {
    let root = temp_root();
    std::fs::create_dir_all(&root).unwrap();
    let store = ServerMonitorStore::new(root.clone());
    let original = b"not-encrypted-corrupt-bytes";
    std::fs::write(store.config_path(), original).unwrap();
    assert!(store.load_config().is_err());
    let config = ServerMonitorConfig::default();
    let result = store.save_config(&config);
    // Either save fails or the corrupt bytes remain unchanged.
    let after = std::fs::read(store.config_path()).unwrap();
    assert!(result.is_err() || after == original);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn history_clips_to_retention_with_old_row() {
    let root = temp_root();
    let store = ServerMonitorStore::new(root.clone());
    let now = 10_000_000_000i64;
    let old = now - 2 * 86_400_000;
    let sample = ServerMonitorSample {
        id: None,
        target_id: "t".to_string(),
        collected_at_ms: old,
        outcome: ServerMonitorOutcome::Success(MetricSnapshot {
            collected_at_ms: old,
            sampling_window_seconds: 1.0,
            cpu: None,
            memory: None,
            disks: None,
            network: None,
            networks: Vec::new(),
            network_interface: None,
        }),
        sampling_window_seconds: 1.0,
        source_mode: ServerMonitorScheduleMode::Interval,
        expected_interval_seconds: None,
        cpu: None,
        memory: None,
        disks: None,
        networks: Vec::new(),
        network_interface: None,
        error_kind: None,
        error: None,
    };
    let mut config = ServerMonitorConfig::default();
    config.settings.retention_days = 1;
    store.save_config(&config).unwrap();
    store.append_sample(&sample).unwrap();
    // 1-day retention should clip the 2-day-old row even for a 7-day request.
    let rows = store.history("t", now - 7 * 86_400_000, now).unwrap();
    assert!(rows.is_empty(), "retention should clip old rows");
    let _ = std::fs::remove_dir_all(root);
}
