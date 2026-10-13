//! Behavior tests for the per-key daily-reset setting (REQ-001 / AC-001).
//!
//! First slice: the persisted configuration round-trip. Every assertion drives
//! the public persistent boundary — a serde_json operator payload carried
//! through the encrypted `ai_gateway.json` via `storage::write_config` /
//! `storage::read_config` and the serialized `storage::modify_config` writer.
//! The test never calls a private method and never asserts an implementation
//! shape; it only observes the serialized configuration.

use super::{
    auth_header_of, isolated_temp_home, json_mapping, live_candidates, on_disk_key_entry,
    pool_config, pool_key, pool_provider, probe_now, spawn_mock_upstream, temp_home,
    with_temp_home, write_raw_gateway_config, MockReply,
};
use crate::ai_gateway::daily_reset::{
    daily_reset_scanner_started, install_daily_reset_scanner_for_tests_with,
    reset_daily_reset_scanner_for_tests, scan_due_daily_reset_marks, stop_daily_reset_scanner,
};
use crate::ai_gateway::runtime_http::{attempt_non_streaming, CONFIG_UPDATE_EVENTS};
use crate::ai_gateway::selection::{find_key_probe_candidate, select_usable_key};
use crate::ai_gateway::storage::{config_path, modify_config, read_config, write_config};
use crate::ai_gateway::types_config::{
    GatewayConfig, GatewayKey, GatewayUpstreamProvider, KeyFailureKind, ModelMapping, ModelPrice,
    UpstreamKey,
};
use crate::ai_gateway::{UsageResult, AI_GATEWAY_CONFIG_UPDATED_EVENT};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// AC-001 / REQ-001: each upstream key keeps an independent `daily_reset`
/// boolean through the encrypted configuration round-trip, a record stored
/// without the field reads as off, and a runtime mark update preserves the
/// setting.
#[test]
fn daily_reset_setting_round_trips_independently_through_encrypted_config() {
    with_temp_home("daily-reset-setting", |_home| {
        // The operator payload: one daily-reset key, one explicitly off key and
        // one legacy record that predates the field.
        let payload = json!({
            "schema_version": 3,
            "providers": [{
                "id": "p1",
                "name": "Provider p1",
                "base_url": "https://api.example.com/v1",
                "protocol": "chat_completions",
                "enabled": true,
                "keys": [
                    {
                        "id": "k-daily",
                        "name": "Daily",
                        "value": "SAFE_FIXTURE_daily_key_value",
                        "enabled": true,
                        "daily_reset": true
                    },
                    {
                        "id": "k-off",
                        "name": "Off",
                        "value": "SAFE_FIXTURE_off_key_value",
                        "enabled": true,
                        "daily_reset": false
                    },
                    {
                        "id": "k-legacy",
                        "name": "Legacy",
                        "value": "SAFE_FIXTURE_legacy_key_value",
                        "enabled": true
                    }
                ]
            }]
        });
        let config: GatewayConfig =
            serde_json::from_value(payload).expect("operator payload must deserialize");

        write_config(&config).expect("persist the configuration");

        // The credentials land in the encrypted profile file, never plaintext.
        let raw = fs::read_to_string(config_path().expect("config path")).expect("read raw config");
        assert!(
            raw.trim_start().starts_with("v2:"),
            "the persisted gateway configuration must stay encrypted"
        );
        assert!(
            !raw.contains("SAFE_FIXTURE_daily_key_value"),
            "the upstream credential must never be stored in plaintext"
        );

        // Loaded settings are independent and the legacy record defaults to off.
        let loaded = read_config().expect("read the persisted configuration");
        let serialized = serde_json::to_value(&loaded).expect("serialize loaded config");
        let keys = provider_keys(&serialized, "p1");
        assert_eq!(
            key_setting(&keys, "k-daily"),
            Some(true),
            "the daily-reset=true setting must survive the encrypted round-trip"
        );
        assert_eq!(
            key_setting(&keys, "k-off"),
            Some(false),
            "an explicit daily-reset=false setting must survive independently"
        );
        assert_eq!(
            key_setting(&keys, "k-legacy"),
            Some(false),
            "a record stored without the field must read as off"
        );

        // A runtime quota mark update must preserve each setting.
        modify_config::<()>(|config| {
            let provider = config
                .providers
                .iter_mut()
                .find(|provider| provider.id == "p1")
                .expect("the seeded provider must remain");
            let key = provider
                .keys
                .iter_mut()
                .find(|key| key.id == "k-daily")
                .expect("the seeded daily key must remain");
            key.auto_marked = true;
            key.failure_kind = Some(KeyFailureKind::Quota);
            key.marked_at = Some(1_700_000_000);
            key.reason = Some("quota exhausted".to_string());
            Ok((true, ()))
        })
        .expect("persist the runtime mark update");

        let reloaded = read_config().expect("read the marked configuration");
        let serialized = serde_json::to_value(&reloaded).expect("serialize reloaded config");
        let keys = provider_keys(&serialized, "p1");
        assert_eq!(
            key_setting(&keys, "k-daily"),
            Some(true),
            "a runtime mark update must preserve the daily-reset setting"
        );
        assert_eq!(
            key_setting(&keys, "k-off"),
            Some(false),
            "a runtime mark update must leave the other key's setting unchanged"
        );
        let marked = key_object(&keys, "k-daily");
        assert_eq!(marked["auto_marked"], json!(true));
        assert_eq!(marked["failure_kind"], json!("quota"));
        assert_eq!(marked["marked_at"], json!(1_700_000_000u64));
    });
}

/// The serialized key pool of one provider from a serialized [`GatewayConfig`].
fn provider_keys(serialized: &Value, provider_id: &str) -> Vec<Value> {
    serialized["providers"]
        .as_array()
        .expect("providers array")
        .iter()
        .find(|provider| provider["id"] == json!(provider_id))
        .expect("the provider must persist")["keys"]
        .as_array()
        .expect("keys array")
        .clone()
}

/// One serialized key entry by its stable id.
fn key_object(keys: &[Value], key_id: &str) -> Value {
    keys.iter()
        .find(|key| key["id"] == json!(key_id))
        .expect("the key must persist")
        .clone()
}

/// The `daily_reset` setting of one serialized key, or `None` when the field is
/// absent from the serialized configuration.
fn key_setting(keys: &[Value], key_id: &str) -> Option<bool> {
    key_object(keys, key_id)
        .get("daily_reset")
        .and_then(Value::as_bool)
}

// ---------------------------------------------------------------------------
// Slice B (REQ-003 / AC-003, REQ-004 / AC-006): UTC+8 day eligibility of a
// quota-marked key for normal selection and the half-open probe.
//
// All timestamps are independent literal UNIX seconds with their UTC+8 and UTC
// wall times documented; no expected value is derived by re-implementing the
// eligibility rule.
// ---------------------------------------------------------------------------

/// 2026-10-13 14:00:00 UTC+8 (2026-10-13T06:00:00Z).
const T_2026_10_13_1400_UTC8: u64 = 1_791_871_200;
/// 2026-10-13 23:59:59 UTC+8 (2026-10-13T15:59:59Z).
const T_2026_10_13_235959_UTC8: u64 = 1_791_907_199;
/// 2026-10-14 00:00:00 UTC+8 (2026-10-13T16:00:00Z), the next day boundary.
const T_2026_10_14_0000_UTC8: u64 = 1_791_907_200;
/// 2026-10-13 22:00:00 UTC+8 (2026-10-13T14:00:00Z), eight hours after the
/// 14:00 mark and therefore past the legacy 1800-second TTL.
const T_2026_10_13_2200_UTC8: u64 = 1_791_900_000;
/// 2026-10-15 10:00:00 UTC+8 (2026-10-15T02:00:00Z), a future-dated mark.
const T_2026_10_15_1000_UTC8: u64 = 1_792_029_600;
/// 2026-10-13 12:00:00 UTC+8 (2026-10-13T04:00:00Z), the legacy-rule reference
/// `now` for the 1800 s / 60 s boundary cases.
const T_2026_10_13_1200_UTC8: u64 = 1_791_864_000;
/// 2026-10-13 11:30:00 UTC+8 — exactly 1800 s before the legacy reference now.
const T_LEGACY_TTL_EXACT: u64 = 1_791_862_200;
/// 2026-10-13 11:29:59 UTC+8 — one second below the 1800 s TTL.
const T_LEGACY_TTL_BELOW: u64 = 1_791_862_201;
/// 2026-10-13 11:59:00 UTC+8 — exactly 60 s before the legacy reference now.
const T_LEGACY_PROBE_EXACT: u64 = 1_791_863_940;
/// 2026-10-13 11:58:59 UTC+8 — one second below the 60 s probe cooldown.
const T_LEGACY_PROBE_BELOW: u64 = 1_791_863_941;

struct EligibilityCase {
    name: &'static str,
    keys: Vec<UpstreamKey>,
    now: u64,
    attempted: &'static [&'static str],
    expect_selected: Option<&'static str>,
    expect_probe: Option<&'static str>,
}

fn enabled_key(id: &str) -> UpstreamKey {
    UpstreamKey {
        id: id.to_string(),
        name: id.to_string(),
        value: format!("SAFE_FIXTURE_{id}"),
        enabled: true,
        ..UpstreamKey::default()
    }
}

fn daily_quota_key(id: &str, marked_at: Option<u64>) -> UpstreamKey {
    UpstreamKey {
        daily_reset: true,
        auto_marked: true,
        failure_kind: Some(KeyFailureKind::Quota),
        marked_at,
        reason: Some("quota exhausted".to_string()),
        ..enabled_key(id)
    }
}

fn legacy_quota_key(id: &str, marked_at: u64) -> UpstreamKey {
    UpstreamKey {
        auto_marked: true,
        failure_kind: Some(KeyFailureKind::Quota),
        marked_at: Some(marked_at),
        reason: Some("quota exhausted".to_string()),
        ..enabled_key(id)
    }
}

/// AC-003 / AC-006: for a quota-marked key with `daily_reset = true` the
/// explicit `now`'s fixed UTC+8 natural day decides eligibility — a same-day mark
/// is excluded from both selection and the probe however old it is, and the mark
/// becomes selectable exactly at the next UTC+8 midnight even at one second of
/// age. `daily_reset = false` keeps the legacy inclusive 1800 s TTL and 60 s
/// probe boundaries, and auth/missing-time/user-disabled keys never recover.
#[test]
fn daily_reset_quota_key_eligibility_follows_utc8_day_boundaries() {
    let cases = vec![
        EligibilityCase {
            name: "daily same-day mark older than 1800s is excluded from selection and probe",
            keys: vec![daily_quota_key("k-daily", Some(T_2026_10_13_1400_UTC8))],
            now: T_2026_10_13_2200_UTC8,
            attempted: &[],
            expect_selected: None,
            expect_probe: None,
        },
        EligibilityCase {
            name: "daily mark at 23:59:59 is still excluded one second before midnight",
            keys: vec![daily_quota_key("k-daily", Some(T_2026_10_13_1400_UTC8))],
            now: T_2026_10_13_235959_UTC8,
            attempted: &[],
            expect_selected: None,
            expect_probe: None,
        },
        EligibilityCase {
            name: "daily mark at 23:59:59 is selectable at the next UTC+8 midnight despite one second of age",
            keys: vec![daily_quota_key("k-daily", Some(T_2026_10_13_235959_UTC8))],
            now: T_2026_10_14_0000_UTC8,
            attempted: &[],
            expect_selected: Some("k-daily"),
            expect_probe: None,
        },
        EligibilityCase {
            name: "future-dated daily mark is never recovered early",
            keys: vec![daily_quota_key("k-daily", Some(T_2026_10_15_1000_UTC8))],
            now: T_2026_10_13_2200_UTC8,
            attempted: &[],
            expect_selected: None,
            expect_probe: None,
        },
        EligibilityCase {
            name: "daily quota mark without a timestamp never recovers",
            keys: vec![daily_quota_key("k-daily", None)],
            now: T_2026_10_14_0000_UTC8,
            attempted: &[],
            expect_selected: None,
            expect_probe: None,
        },
        EligibilityCase {
            name: "authentication mark on a daily key never expires",
            keys: vec![UpstreamKey {
                daily_reset: true,
                auto_marked: true,
                failure_kind: Some(KeyFailureKind::Authentication),
                marked_at: Some(T_2026_10_13_1400_UTC8),
                reason: Some("unauthorized".to_string()),
                ..enabled_key("k-auth")
            }],
            now: T_2026_10_14_0000_UTC8,
            attempted: &[],
            expect_selected: None,
            expect_probe: None,
        },
        EligibilityCase {
            name: "user-disabled daily key stays excluded",
            keys: vec![UpstreamKey {
                enabled: false,
                ..daily_quota_key("k-disabled", Some(T_2026_10_13_1400_UTC8))
            }],
            now: T_2026_10_13_2200_UTC8,
            attempted: &[],
            expect_selected: None,
            expect_probe: None,
        },
        EligibilityCase {
            name: "legacy key at exactly 1800s keeps inclusive TTL selection and probe eligibility",
            keys: vec![legacy_quota_key("k-legacy", T_LEGACY_TTL_EXACT)],
            now: T_2026_10_13_1200_UTC8,
            attempted: &[],
            expect_selected: Some("k-legacy"),
            expect_probe: Some("k-legacy"),
        },
        EligibilityCase {
            name: "legacy key one second below the TTL is not selectable but stays probe eligible",
            keys: vec![legacy_quota_key("k-legacy", T_LEGACY_TTL_BELOW)],
            now: T_2026_10_13_1200_UTC8,
            attempted: &[],
            expect_selected: None,
            expect_probe: Some("k-legacy"),
        },
        EligibilityCase {
            name: "legacy key at exactly 60s is probe eligible",
            keys: vec![legacy_quota_key("k-legacy", T_LEGACY_PROBE_EXACT)],
            now: T_2026_10_13_1200_UTC8,
            attempted: &[],
            expect_selected: None,
            expect_probe: Some("k-legacy"),
        },
        EligibilityCase {
            name: "legacy key one second below the probe cooldown is not probe eligible",
            keys: vec![legacy_quota_key("k-legacy", T_LEGACY_PROBE_BELOW)],
            now: T_2026_10_13_1200_UTC8,
            attempted: &[],
            expect_selected: None,
            expect_probe: None,
        },
        EligibilityCase {
            name: "list order: a healthy key after an excluded same-day daily key is selected",
            keys: vec![
                daily_quota_key("k-daily", Some(T_2026_10_13_1400_UTC8)),
                enabled_key("k-healthy"),
            ],
            now: T_2026_10_13_2200_UTC8,
            attempted: &[],
            expect_selected: Some("k-healthy"),
            expect_probe: None,
        },
        EligibilityCase {
            name: "attempted key advances to the next usable key in list order",
            keys: vec![enabled_key("k-a"), enabled_key("k-b")],
            now: T_2026_10_13_1200_UTC8,
            attempted: &["k-a"],
            expect_selected: Some("k-b"),
            expect_probe: None,
        },
        EligibilityCase {
            name: "an attempted healthy key never falls back to an excluded same-day daily key",
            keys: vec![
                daily_quota_key("k-daily", Some(T_2026_10_13_1400_UTC8)),
                enabled_key("k-healthy"),
            ],
            now: T_2026_10_13_2200_UTC8,
            attempted: &["k-healthy"],
            expect_selected: None,
            expect_probe: None,
        },
    ];

    let mut failures = Vec::new();
    for case in cases {
        let EligibilityCase {
            name,
            keys,
            now,
            attempted,
            expect_selected,
            expect_probe,
        } = case;
        let provider = GatewayUpstreamProvider {
            id: "p".to_string(),
            name: "Provider p".to_string(),
            base_url: "https://api.example.com/v1".to_string(),
            keys,
            ..GatewayUpstreamProvider::default()
        };
        let attempted: HashSet<String> = attempted.iter().map(|id| (*id).to_string()).collect();
        let selected = select_usable_key(&provider, now, &attempted).map(|key| key.id.as_str());
        let probe = find_key_probe_candidate(&provider, now).map(|key| key.id.as_str());
        if selected != expect_selected {
            failures.push(format!(
                "selection [{name}]: expected {expect_selected:?}, got {selected:?}"
            ));
        }
        if probe != expect_probe {
            failures.push(format!(
                "probe [{name}]: expected {expect_probe:?}, got {probe:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "daily-reset eligibility mismatches:\n{}",
        failures.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Slice C (REQ-003 / AC-004, REQ-004 / AC-007): the process-owned daily-reset
// due scan. This drives the intended production scan entry with an explicit
// `now` and a real mock-runtime `AppHandle`, observes the encrypted
// configuration on disk and the existing test-observable configuration-update
// seam, never starts the listener and never mocks recovery.
// ---------------------------------------------------------------------------

/// 2026-10-12 14:00:00 UTC+8 (2026-10-12T06:00:00Z) — a past quota mark used for
/// the auth/disabled exclusions.
const T_2026_10_12_1400_UTC8: u64 = 1_791_784_800;
/// 2026-10-14 09:00:00 UTC+8 (2026-10-14T01:00:00Z) — a same-day quota mark at
/// the recovery `now`, which must stay marked.
const T_2026_10_14_0900_UTC8: u64 = 1_791_939_600;
/// 2026-10-14 10:00:00 UTC+8 (2026-10-14T02:00:00Z) — the recovery scan `now`,
/// one UTC+8 day after the eligible mark.
const T_2026_10_14_1000_UTC8: u64 = 1_791_943_200;

fn recorded_config_update_events() -> Vec<String> {
    CONFIG_UPDATE_EVENTS.with(|events| events.borrow().clone())
}

fn clear_config_update_events() {
    CONFIG_UPDATE_EVENTS.with(|events| events.borrow_mut().clear());
}

fn key_entry<'a>(provider: &'a GatewayUpstreamProvider, id: &str) -> &'a UpstreamKey {
    provider
        .keys
        .iter()
        .find(|key| key.id == id)
        .unwrap_or_else(|| panic!("the key {id} must persist"))
}

/// AC-004 / AC-007: one production due scan over the latest serialized encrypted
/// configuration recovers exactly the day-due daily-reset quota marks, clears
/// only their runtime fields, preserves user intent, unrelated configuration and
/// exclusions, and emits the existing configuration-update event through the
/// supplied handle only after a successful persisted change. A scan with nothing
/// due neither writes nor emits.
#[test]
fn daily_reset_due_scan_recovers_due_marks_and_emits_after_persisting() {
    let _home = temp_home("daily-reset-due-scan");
    let app = tauri::test::mock_app();

    let mut config = GatewayConfig::default();
    config.enabled = true;
    config.template_auto_refresh_minutes = 0;
    config.usage_retention_days = 30;
    config.keys.push(GatewayKey {
        id: "local-k1".to_string(),
        label: "Local".to_string(),
        value: "SAFE_FIXTURE_local_gateway_key".to_string(),
        enabled: true,
        created_at: 1,
    });
    config.default_key_id = Some("local-k1".to_string());
    config.model_prices.push(ModelPrice {
        provider_id: Some("p-mixed".to_string()),
        upstream_model: "remote-due".to_string(),
        input: 1.5,
        cache_read: 0.0,
        cache_write: 0.0,
        output: 2.0,
        off_peaks: Vec::new(),
    });
    config.providers.push(GatewayUpstreamProvider {
        id: "p-mixed".to_string(),
        name: "Provider Mixed".to_string(),
        base_url: "https://api.example.com/v1".to_string(),
        default_model: Some("remote-default".to_string()),
        mappings: vec![ModelMapping {
            local_model: "local-a".to_string(),
            upstream_model: "remote-due".to_string(),
            ..ModelMapping::default()
        }],
        keys: vec![
            daily_quota_key("k-due", Some(T_2026_10_13_235959_UTC8)),
            daily_quota_key("k-same-day", Some(T_2026_10_14_0900_UTC8)),
            daily_quota_key("k-future", Some(T_2026_10_15_1000_UTC8)),
            legacy_quota_key("k-legacy", T_2026_10_13_1400_UTC8),
            UpstreamKey {
                daily_reset: true,
                auto_marked: true,
                failure_kind: Some(KeyFailureKind::Authentication),
                marked_at: Some(T_2026_10_12_1400_UTC8),
                reason: Some("unauthorized".to_string()),
                ..enabled_key("k-auth")
            },
            UpstreamKey {
                enabled: false,
                ..daily_quota_key("k-disabled", Some(T_2026_10_12_1400_UTC8))
            },
            daily_quota_key("k-no-time", None),
            enabled_key("k-healthy"),
        ],
        ..GatewayUpstreamProvider::default()
    });
    config.providers.push(GatewayUpstreamProvider {
        id: "p-other".to_string(),
        name: "Provider Other".to_string(),
        base_url: "https://other.example.com/v1".to_string(),
        default_model: Some("remote-other".to_string()),
        mappings: vec![ModelMapping {
            local_model: "local-other".to_string(),
            upstream_model: "remote-other".to_string(),
            ..ModelMapping::default()
        }],
        keys: vec![enabled_key("k-other")],
        weight: 7,
        tags: vec!["tag-a".to_string()],
        ..GatewayUpstreamProvider::default()
    });
    write_config(&config).expect("seed the encrypted configuration");

    let path = config_path().expect("config path");

    // No-op: at 22:00 UTC+8 every seeded daily mark is on this same day or later.
    let before = fs::read(&path).expect("read the seeded bytes");
    clear_config_update_events();
    let recovered_none = scan_due_daily_reset_marks(app.handle(), T_2026_10_13_2200_UTC8)
        .expect("the no-op scan must succeed");
    assert_eq!(
        recovered_none, 0,
        "no key is due before the UTC+8 day boundary"
    );
    assert!(
        recorded_config_update_events().is_empty(),
        "a no-op scan must not emit a configuration-update event"
    );
    assert_eq!(
        fs::read(&path).expect("reread the bytes"),
        before,
        "a no-op scan must not rewrite the configuration"
    );

    // Recovery: at 10:00 the next UTC+8 day only `k-due` has passed its boundary.
    clear_config_update_events();
    let recovered = scan_due_daily_reset_marks(app.handle(), T_2026_10_14_1000_UTC8)
        .expect("the due scan must succeed");
    assert_eq!(
        recovered, 1,
        "exactly the day-due daily-reset key must recover"
    );
    assert_eq!(
        recorded_config_update_events(),
        vec![AI_GATEWAY_CONFIG_UPDATED_EVENT.to_string()],
        "a successful persisted recovery must emit exactly one config-update event"
    );

    let loaded = read_config().expect("read the recovered configuration");
    let provider = loaded
        .providers
        .iter()
        .find(|provider| provider.id == "p-mixed")
        .expect("the mixed provider must persist");

    let due = key_entry(provider, "k-due");
    assert!(!due.auto_marked, "the due quota mark must be cleared");
    assert_eq!(due.failure_kind, None, "the failure kind must be cleared");
    assert_eq!(due.marked_at, None, "the mark time must be cleared");
    assert_eq!(due.reason, None, "the mark reason must be cleared");
    assert!(
        due.daily_reset,
        "the daily-reset setting must survive recovery"
    );
    assert!(
        due.enabled,
        "the user's enabled intent must survive recovery"
    );

    assert!(
        key_entry(provider, "k-same-day").auto_marked
            && key_entry(provider, "k-same-day").marked_at == Some(T_2026_10_14_0900_UTC8),
        "a same-UTC+8-day mark must not recover"
    );
    assert!(
        key_entry(provider, "k-future").auto_marked
            && key_entry(provider, "k-future").marked_at == Some(T_2026_10_15_1000_UTC8),
        "a future-dated mark must never recover early"
    );
    assert!(
        key_entry(provider, "k-legacy").auto_marked && !key_entry(provider, "k-legacy").daily_reset,
        "a daily_reset=false key must not be touched by the daily scan"
    );
    assert!(
        key_entry(provider, "k-auth").failure_kind == Some(KeyFailureKind::Authentication)
            && key_entry(provider, "k-auth").auto_marked,
        "an authentication mark must never clear through daily recovery"
    );
    assert!(
        !key_entry(provider, "k-disabled").enabled && key_entry(provider, "k-disabled").auto_marked,
        "a user-disabled key must stay disabled and marked"
    );
    assert!(
        key_entry(provider, "k-no-time").auto_marked
            && key_entry(provider, "k-no-time").marked_at.is_none(),
        "a quota mark without a timestamp must stay"
    );
    assert!(
        !key_entry(provider, "k-healthy").auto_marked,
        "an unmarked healthy key must stay untouched"
    );

    // Unrelated configuration is preserved byte-for-field.
    assert_eq!(loaded.usage_retention_days, 30);
    assert_eq!(loaded.default_key_id.as_deref(), Some("local-k1"));
    assert_eq!(loaded.keys.len(), 1);
    assert_eq!(loaded.keys[0].value, "SAFE_FIXTURE_local_gateway_key");
    assert_eq!(loaded.model_prices.len(), 1);
    assert_eq!(
        loaded.model_prices[0].provider_id.as_deref(),
        Some("p-mixed")
    );
    let other = loaded
        .providers
        .iter()
        .find(|provider| provider.id == "p-other")
        .expect("the unrelated provider must persist");
    assert_eq!(other.base_url, "https://other.example.com/v1");
    assert_eq!(other.default_model.as_deref(), Some("remote-other"));
    assert_eq!(other.weight, 7);
    assert_eq!(other.mappings.len(), 1);
    assert!(!key_entry(other, "k-other").auto_marked);

    // The recovered configuration stays encrypted; no credential is plaintext.
    let raw = fs::read_to_string(&path).expect("read the raw recovered configuration");
    assert!(
        raw.trim_start().starts_with("v2:"),
        "the recovered configuration must stay encrypted"
    );
    assert!(
        !raw.contains("SAFE_FIXTURE_local_gateway_key"),
        "no gateway credential may be stored in plaintext"
    );

    // A repeat scan with nothing due neither writes nor emits.
    let after_recovery = fs::read(&path).expect("read the recovered bytes");
    clear_config_update_events();
    let recovered_again = scan_due_daily_reset_marks(app.handle(), T_2026_10_14_1000_UTC8)
        .expect("the repeat scan must succeed");
    assert_eq!(recovered_again, 0, "no key remains due after recovery");
    assert!(
        recorded_config_update_events().is_empty(),
        "a repeat no-op scan must not emit a configuration-update event"
    );
    assert_eq!(
        fs::read(&path).expect("reread the bytes"),
        after_recovery,
        "a repeat no-op scan must not rewrite the configuration"
    );
}

// ---------------------------------------------------------------------------
// Slice D (REQ-003 / AC-004): the process-owned scanner lifecycle. The test
// installs the SAME production loop through the explicit tick/clock test seam,
// drives it with an atomic UNIX clock and a shortened period instead of real
// 60-second sleeps, and observes only the persisted encrypted configuration and
// the handle-backed existing configuration-update event. It never starts the
// listener and keeps template auto-refresh disabled.
// ---------------------------------------------------------------------------

/// 2026-10-14 23:00:00 UTC+8 (2026-10-14T15:00:00Z) — a mark one UTC+8 day
/// before the post-shutdown re-install `now`.
const T_2026_10_14_2300_UTC8: u64 = 1_791_990_000;

fn persisted_key_is_cleared(provider_id: &str, key_id: &str) -> bool {
    let loaded = read_config().expect("read the configuration");
    loaded
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .and_then(|provider| provider.keys.iter().find(|key| key.id == key_id))
        .map(|key| !key.auto_marked && key.marked_at.is_none() && key.failure_kind.is_none())
        .unwrap_or(false)
}

fn wait_until(label: &str, timeout: Duration, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {label}");
}

/// AC-004: the installed process-owned scanner runs the immediate startup scan
/// exactly once, keeps repeated installation a no-op, catches up through its
/// periodic pass with a fresh wall clock across a UTC+8 day boundary, and stops
/// idempotently without running further passes. Recovery is observed only through
/// the persisted encrypted configuration and the handle-backed existing event.
#[test]
fn daily_reset_scanner_startup_periodic_catch_up_and_idempotent_stop() {
    let _home = temp_home("daily-reset-scanner-lifecycle");
    let app = tauri::test::mock_app();
    let clock = Arc::new(AtomicU64::new(T_2026_10_13_2200_UTC8));

    // Seed: a key due at startup, a key that catches up one day later, a key for
    // the post-stop re-install, an authentication mark, an unrelated provider and
    // disabled template auto-refresh. No listener is ever started.
    let mut config = GatewayConfig::default();
    config.enabled = true;
    config.template_auto_refresh_minutes = 0;
    config.usage_retention_days = 30;
    config.providers.push(GatewayUpstreamProvider {
        id: "p-scan".to_string(),
        name: "Provider Scan".to_string(),
        base_url: "https://api.example.com/v1".to_string(),
        keys: vec![
            daily_quota_key("k-startup-due", Some(T_2026_10_12_1400_UTC8)),
            daily_quota_key("k-catch-up", Some(T_2026_10_13_235959_UTC8)),
            daily_quota_key("k-post-stop", Some(T_2026_10_14_2300_UTC8)),
            UpstreamKey {
                daily_reset: true,
                auto_marked: true,
                failure_kind: Some(KeyFailureKind::Authentication),
                marked_at: Some(T_2026_10_12_1400_UTC8),
                reason: Some("unauthorized".to_string()),
                ..enabled_key("k-auth")
            },
        ],
        ..GatewayUpstreamProvider::default()
    });
    config.providers.push(GatewayUpstreamProvider {
        id: "p-other".to_string(),
        name: "Provider Other".to_string(),
        base_url: "https://other.example.com/v1".to_string(),
        mappings: vec![ModelMapping {
            local_model: "local-other".to_string(),
            upstream_model: "remote-other".to_string(),
            ..ModelMapping::default()
        }],
        weight: 7,
        ..GatewayUpstreamProvider::default()
    });
    write_config(&config).expect("seed the encrypted configuration");

    reset_daily_reset_scanner_for_tests();
    assert!(
        !daily_reset_scanner_started(),
        "the scanner must start uninstalled"
    );

    // Installation runs exactly one immediate startup scan at the current clock.
    clear_config_update_events();
    let startup_clock = clock.clone();
    let installed = install_daily_reset_scanner_for_tests_with(
        app.handle().clone(),
        Duration::from_millis(30),
        move || startup_clock.load(Ordering::SeqCst),
    );
    assert!(installed, "the first installation must install the scanner");
    assert!(
        daily_reset_scanner_started(),
        "the scanner must report installed"
    );
    assert!(
        persisted_key_is_cleared("p-scan", "k-startup-due"),
        "the immediate startup scan must recover the day-due key"
    );
    assert!(
        !persisted_key_is_cleared("p-scan", "k-catch-up"),
        "a same-day key must not recover at startup"
    );
    assert_eq!(
        recorded_config_update_events(),
        vec![AI_GATEWAY_CONFIG_UPDATED_EVENT.to_string()],
        "the startup recovery must emit exactly one handle-backed config-update event"
    );

    // A repeated installation is a no-op and runs no second startup scan.
    let repeat_clock = clock.clone();
    let reinstalled = install_daily_reset_scanner_for_tests_with(
        app.handle().clone(),
        Duration::from_millis(30),
        move || repeat_clock.load(Ordering::SeqCst),
    );
    assert!(!reinstalled, "a repeated installation must be a no-op");
    assert!(daily_reset_scanner_started());
    assert_eq!(
        recorded_config_update_events(),
        vec![AI_GATEWAY_CONFIG_UPDATED_EVENT.to_string()],
        "a repeated installation must not run another startup scan"
    );

    // The periodic pass reads the fresh clock and catches up across the boundary.
    clock.store(T_2026_10_14_1000_UTC8, Ordering::SeqCst);
    wait_until(
        "the periodic catch-up recovery",
        Duration::from_secs(5),
        || persisted_key_is_cleared("p-scan", "k-catch-up"),
    );
    assert!(
        !persisted_key_is_cleared("p-scan", "k-post-stop"),
        "a later day key must stay marked during the catch-up pass"
    );
    let after_catch_up = read_config().expect("read the catch-up configuration");
    let scan_provider = after_catch_up
        .providers
        .iter()
        .find(|provider| provider.id == "p-scan")
        .expect("the scan provider must persist");
    assert_eq!(
        key_entry(scan_provider, "k-auth").failure_kind,
        Some(KeyFailureKind::Authentication),
        "the authentication mark must never clear through the scanner"
    );

    // Shutdown stops the scanner idempotently and releases the installation.
    stop_daily_reset_scanner();
    stop_daily_reset_scanner();
    assert!(
        !daily_reset_scanner_started(),
        "shutdown must clear the installed scanner"
    );

    // A stopped scanner runs no further pass even after the day boundary.
    std::thread::sleep(Duration::from_millis(150));
    clock.store(T_2026_10_15_1000_UTC8, Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(250));
    assert!(
        !persisted_key_is_cleared("p-scan", "k-post-stop"),
        "a stopped scanner must not recover another key"
    );

    // Re-installing runs a fresh immediate startup scan at the current clock.
    let reinstall_clock = clock.clone();
    let re_installed = install_daily_reset_scanner_for_tests_with(
        app.handle().clone(),
        Duration::from_millis(30),
        move || reinstall_clock.load(Ordering::SeqCst),
    );
    assert!(re_installed, "a stopped scanner must be re-installable");
    assert!(
        persisted_key_is_cleared("p-scan", "k-post-stop"),
        "the re-installed startup scan must catch up"
    );

    // Unrelated configuration survives every pass.
    let loaded = read_config().expect("read the final configuration");
    assert_eq!(loaded.template_auto_refresh_minutes, 0);
    assert_eq!(loaded.usage_retention_days, 30);
    let other = loaded
        .providers
        .iter()
        .find(|provider| provider.id == "p-other")
        .expect("the unrelated provider must persist");
    assert_eq!(other.base_url, "https://other.example.com/v1");
    assert_eq!(other.weight, 7);
    assert_eq!(other.mappings.len(), 1);

    stop_daily_reset_scanner();
    reset_daily_reset_scanner_for_tests();
    assert!(!daily_reset_scanner_started());
}

// ---------------------------------------------------------------------------
// Slice E (REQ-004 / AC-007): a real encrypted persistence failure during a due
// scan. The test induces a genuine filesystem write failure (read-only profile
// directory, restored by a guard) and proves the scan propagates the error,
// leaves the previous complete ciphertext intact and emits no event; removing
// the failure lets a later scan recover the still-due key.
// ---------------------------------------------------------------------------

#[cfg(unix)]
use std::path::PathBuf;

/// Read-only profile-directory guard that restores the original permissions on
/// drop, so the temp home can be removed and a later write succeeds even when an
/// assertion panics. Mirrors the migration/AC-005 permission guard.
#[cfg(unix)]
struct ReadOnlyAppDirGuard {
    dir: PathBuf,
    original: fs::Permissions,
}

#[cfg(unix)]
impl ReadOnlyAppDirGuard {
    fn new(dir: PathBuf) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let original = fs::metadata(&dir)
            .expect("app directory metadata")
            .permissions();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o500))
            .expect("make the app directory read-only");
        Self { dir, original }
    }
}

#[cfg(unix)]
impl Drop for ReadOnlyAppDirGuard {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.dir, self.original.clone());
    }
}

/// AC-007: a real encrypted persistence failure during a due scan propagates as
/// an error, leaves the previous complete ciphertext on disk, clears nothing and
/// emits no configuration-update event; once the failure is removed a later scan
/// recovers the still-due key and emits exactly one event.
#[cfg(unix)]
#[test]
fn daily_reset_scan_propagates_a_real_persistence_failure_and_recovers_later() {
    let _home = temp_home("daily-reset-persistence-failure");
    let app = tauri::test::mock_app();

    let mut config = GatewayConfig::default();
    config.template_auto_refresh_minutes = 0;
    config.providers.push(GatewayUpstreamProvider {
        id: "p-fail".to_string(),
        name: "Provider Fail".to_string(),
        base_url: "https://api.example.com/v1".to_string(),
        keys: vec![daily_quota_key("k-due", Some(T_2026_10_12_1400_UTC8))],
        ..GatewayUpstreamProvider::default()
    });
    write_config(&config).expect("seed the encrypted configuration");

    let path = config_path().expect("config path");
    let app_dir = path.parent().expect("config directory").to_path_buf();
    let before = fs::read(&path).expect("read the seeded ciphertext");
    assert!(
        !before.is_empty(),
        "the seeded configuration must be non-empty"
    );

    // Force the real atomic temp-file write to fail; the guard restores the
    // directory even if an assertion below panics.
    clear_config_update_events();
    let readonly_guard = ReadOnlyAppDirGuard::new(app_dir);

    let failure = scan_due_daily_reset_marks(app.handle(), T_2026_10_13_2200_UTC8)
        .expect_err("a real encrypted write failure must propagate as an error");
    assert!(
        !failure.is_empty(),
        "the persistence failure must carry an actionable message"
    );
    assert!(
        recorded_config_update_events().is_empty(),
        "a failed write must emit no configuration-update event"
    );
    assert_eq!(
        fs::read(&path).expect("reread the ciphertext"),
        before,
        "the previous complete ciphertext must stay intact after a failed write"
    );
    assert!(
        !persisted_key_is_cleared("p-fail", "k-due"),
        "the failed write must not clear the persisted mark"
    );

    // Remove the real failure; the still-due key recovers on the next scan.
    drop(readonly_guard);
    clear_config_update_events();
    let recovered = scan_due_daily_reset_marks(app.handle(), T_2026_10_13_2200_UTC8)
        .expect("the later scan must succeed once the directory is writable");
    assert_eq!(
        recovered, 1,
        "the still-due key must recover after the failure is removed"
    );
    assert_eq!(
        recorded_config_update_events(),
        vec![AI_GATEWAY_CONFIG_UPDATED_EVENT.to_string()],
        "the successful later scan must emit exactly one config-update event"
    );
    assert!(
        persisted_key_is_cleared("p-fail", "k-due"),
        "the later scan must clear the recovered mark"
    );

    let raw = fs::read_to_string(&path).expect("read the raw recovered configuration");
    assert!(
        raw.trim_start().starts_with("v2:"),
        "the recovered configuration must stay encrypted"
    );
    assert!(
        !raw.contains("SAFE_FIXTURE_k-due"),
        "no upstream credential may be stored in plaintext"
    );
}

// ---------------------------------------------------------------------------
// Slice F (REQ-003 / AC-005): a recovered daily-reset key that is first in the
// normal pool order and still quota-exhausted upstream receives a fresh
// current-time mark, the gateway rotates to the next usable key, and the next
// same-day request no longer attempts the daily key. The attempt drives the
// existing production request entry against a real loopback upstream; nothing
// private is mocked.
// ---------------------------------------------------------------------------

fn daily_reset_first_key(marked_at: u64) -> Value {
    json!({
        "id": "key-daily",
        "name": "Daily",
        "value": "SAFE_FIXTURE_daily_upstream",
        "enabled": true,
        "daily_reset": true,
        "auto_marked": true,
        "failure_kind": "quota",
        "marked_at": marked_at,
        "reason": "weekly limit",
    })
}

/// AC-005: a recovered daily-reset key first in pool order that is still
/// quota-exhausted is re-marked at the actual failure time and rotated around,
/// the second usable key serves, mapping health is untouched, and the next
/// same-day request excludes the daily key.
#[tokio::test]
async fn recovered_daily_key_that_still_exhausts_quota_rotates_and_stays_excluded() {
    const QUOTA_MESSAGE: &str = "You've reached your weekly usage limit for your plan. Your limit resets at 2026-09-24T03:30:30.663Z. Please wait for the window to reset or upgrade your plan to continue.";

    let _home = isolated_temp_home("daily-reset-quota-rotation");
    let app = tauri::test::mock_app();
    let (upstream_url, log) = spawn_mock_upstream(move |captured| match auth_header_of(captured) {
        Some("Bearer SAFE_FIXTURE_daily_upstream") => {
            MockReply::Json(429, json!({"error": {"message": QUOTA_MESSAGE}}))
        }
        Some("Bearer SAFE_FIXTURE_second_upstream") => {
            MockReply::Json(200, json!({"id": "served-by-second"}))
        }
        other => MockReply::Json(
            500,
            json!({"error": {"message": format!("unexpected upstream authorization {other:?}")}}),
        ),
    })
    .await;

    // Seed a day-due daily-reset quarantine as the FIRST pool key, then let the
    // production scan recover it before the request is attempted.
    write_raw_gateway_config(&pool_config(
        0,
        vec![pool_provider(
            "p1",
            "Provider One",
            &upstream_url,
            vec![
                daily_reset_first_key(T_2026_10_12_1400_UTC8),
                pool_key("key-second", "Second", "SAFE_FIXTURE_second_upstream", true),
            ],
            vec![json_mapping("local-a", "remote-a", None)],
        )],
    ));
    clear_config_update_events();
    let recovered = scan_due_daily_reset_marks(app.handle(), T_2026_10_13_2200_UTC8)
        .expect("the due scan must recover the seeded daily key");
    assert_eq!(recovered, 1, "the seeded daily key must be recovered");
    assert!(
        persisted_key_is_cleared("p1", "key-daily"),
        "the due scan must clear the daily key's runtime mark before the request"
    );

    // First request: the recovered daily key is selected first, still exhausts
    // quota, is re-marked at the actual failure time and rotates to the second.
    let body = serde_json::to_vec(&json!({"model": "local-a"})).unwrap();
    let before = probe_now();
    let mut attempts = Vec::new();
    let response = attempt_non_streaming(
        &live_candidates("local-a"),
        "/v1/chat/completions",
        &body,
        Some("local-a"),
        &HashMap::new(),
        false,
        None,
        &mut attempts,
    )
    .await;
    let after = probe_now();

    assert_eq!(
        response.status, 200,
        "the second usable key must serve the request"
    );
    assert!(String::from_utf8_lossy(&response.body).contains("served-by-second"));

    let captured = log.lock().unwrap().clone();
    assert_eq!(captured.len(), 2, "one attempt per usable key");
    assert_eq!(
        auth_header_of(&captured[0]),
        Some("Bearer SAFE_FIXTURE_daily_upstream"),
        "the recovered daily key must be attempted first"
    );
    assert_eq!(
        auth_header_of(&captured[1]),
        Some("Bearer SAFE_FIXTURE_second_upstream"),
        "the same request must rotate to the second usable key"
    );
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].status, 429);
    assert_eq!(attempts[0].result, UsageResult::Failure);
    assert_eq!(attempts[1].status, 200);
    assert_eq!(attempts[1].result, UsageResult::Success);

    // The persisted daily key carries a FRESH mark at the actual failure time,
    // not the earlier due-recovery mark, and keeps its daily-reset setting.
    let daily = on_disk_key_entry("p1", "key-daily").expect("the daily key must persist");
    assert_eq!(daily["auto_marked"], Value::Bool(true));
    assert_eq!(daily["failure_kind"], Value::String("quota".to_string()));
    assert_eq!(daily["daily_reset"], Value::Bool(true));
    let marked_at = daily["marked_at"]
        .as_u64()
        .expect("the re-marked key must carry a timestamp");
    assert_ne!(
        marked_at, T_2026_10_12_1400_UTC8,
        "the stale due mark must be replaced"
    );
    assert!(
        before <= marked_at && marked_at <= after,
        "the fresh mark must use the actual failure time: {marked_at} not in [{before}, {after}]"
    );
    let second = on_disk_key_entry("p1", "key-second").expect("the second key must persist");
    assert_eq!(second["auto_marked"], Value::Bool(false));

    // A key-domain quota failure must not touch mapping health.
    let stored = read_config().expect("read the persisted provider state");
    let provider = stored
        .providers
        .iter()
        .find(|provider| provider.id == "p1")
        .expect("the provider must persist");
    assert!(!provider.mappings[0].auto_disabled);
    assert_eq!(provider.mappings[0].consecutive_failures, 0);
    assert_eq!(provider.mappings[0].disabled_reason, None);

    // Second same-day request: the daily key is excluded, so the next usable key
    // is contacted directly and the daily key stays marked.
    let mut second_attempts = Vec::new();
    let second_response = attempt_non_streaming(
        &live_candidates("local-a"),
        "/v1/chat/completions",
        &body,
        Some("local-a"),
        &HashMap::new(),
        false,
        None,
        &mut second_attempts,
    )
    .await;
    assert_eq!(second_response.status, 200);
    let captured = log.lock().unwrap().clone();
    assert_eq!(
        captured.len(),
        3,
        "the next request must not retry the same-day daily key"
    );
    assert_eq!(
        auth_header_of(&captured[2]),
        Some("Bearer SAFE_FIXTURE_second_upstream"),
        "the next request must go straight to the usable key"
    );
    assert_eq!(
        on_disk_key_entry("p1", "key-daily").expect("the daily key must persist")["auto_marked"],
        Value::Bool(true),
        "the daily key must stay excluded for the rest of the UTC+8 day"
    );
}
