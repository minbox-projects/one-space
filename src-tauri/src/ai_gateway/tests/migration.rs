//! Behavior tests for the version-gated one-time legacy migration.
//!
//! Every fixture is built from raw JSON (configuration) or raw SQL (usage
//! database) so this module compiles against both the current API and the
//! cleaned API, and fails behaviorally — never because a future symbol is
//! missing. The configuration is read through [`super::super::storage::read_config`],
//! the only public read boundary, and on-disk shapes are inspected as
//! `serde_json::Value` plus raw bytes.

use super::{config_path, isolated_temp_home, make_temp_dir};
use crate::ai_gateway::migration::USAGE_DB_VERSION;
use crate::ai_gateway::storage::read_config;
use crate::ai_gateway::{
    clear_usage_clock_for_test, set_usage_clock_for_test, LogFilter, ModelPrice, TimeRange,
    UsageLogStore,
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Barrier;

const PROVIDER_RUNTIME_KEYS: [&str; 5] = [
    "auto_disabled",
    "disabled_reason",
    "disabled_at",
    "consecutive_failures",
    "last_error_at",
];

fn write_encrypted_config(value: &Value) {
    let password = crate::crypto::get_or_init_master_password().expect("master password");
    let encrypted = crate::crypto::encrypt(&value.to_string(), &password).expect("encrypt");
    fs::write(config_path().expect("config path"), encrypted).expect("write config");
}

fn raw_config_bytes() -> Vec<u8> {
    fs::read(config_path().expect("config path")).expect("read config bytes")
}

fn decrypted_config_json() -> Value {
    let password = crate::crypto::get_or_init_master_password().expect("master password");
    let raw = fs::read_to_string(config_path().expect("config path")).expect("read config");
    let decrypted = crate::crypto::decrypt(raw.trim(), &password).expect("decrypt");
    serde_json::from_str(&decrypted).expect("config json")
}

/// An encrypted configuration without a schema version that mixes an
/// unreachable global price row, a reachable global price row, a scoped
/// duplicate, a singular `off_peak` window and provider runtime fields.
fn legacy_config_fixture() -> Value {
    json!({
        "enabled": true,
        "providers": [
            {
                "id": "p",
                "name": "Provider P",
                "base_url": "https://p.example.com/v1",
                "api_key": "sk-p",
                "default_model": "remote-default",
                "protocol": "chat_completions",
                "auto_disabled": true,
                "disabled_reason": "legacy provider health",
                "disabled_at": 5,
                "consecutive_failures": 2,
                "last_error_at": 9,
                "mappings": [
                    {"local_model": "local-a", "upstream_model": "remote-a", "enabled": true}
                ]
            },
            {
                "id": "q",
                "name": "Provider Q",
                "base_url": "https://q.example.com/v1",
                "api_key": "sk-q",
                "protocol": "chat_completions",
                "mappings": [
                    {"local_model": "local-q", "upstream_model": "remote-q", "enabled": true}
                ]
            }
        ],
        "model_prices": [
            {"upstream_model": "remote-a", "input": 1.0},
            {"upstream_model": "remote-default", "input": 2.0},
            {"upstream_model": "unreachable-model", "input": 3.0},
            {"provider_id": "p", "upstream_model": "remote-a", "input": 9.0},
            {"provider_id": "ghost", "upstream_model": "remote-a", "input": 5.0},
            {"provider_id": "p", "upstream_model": "unreachable", "input": 5.0},
            {
                "provider_id": "q",
                "upstream_model": "remote-q",
                "input": 4.0,
                "off_peak": {
                    "start_time": "00:30",
                    "end_time": "08:30",
                    "input": 1.0,
                    "cache_read": 0.5,
                    "cache_write": 1.0,
                    "output": 2.0,
                    "days": [1, 2, 3]
                }
            },
            {"provider_id": "q", "upstream_model": "remote-q", "input": 5.0}
        ]
    })
}

/// The former per-read normalization result for price rows: global rows migrate
/// into every reaching provider, unmatched/orphan/unreachable rows are dropped
/// and scoped duplicates keep the first occurrence.
fn assert_price_scoping(config: &crate::ai_gateway::GatewayConfig) {
    assert!(
        config.model_prices.iter().all(|row| row.provider_id.is_some()),
        "every row must be provider-scoped: {:?}",
        config.model_prices
    );
    assert!(
        config
            .model_prices
            .iter()
            .all(|row| row.provider_id.as_deref() != Some("ghost")),
        "a row for a nonexistent provider must be dropped"
    );
    assert!(
        config
            .model_prices
            .iter()
            .all(|row| row.upstream_model != "unreachable-model"),
        "an unmatched global row must be deleted"
    );
    assert!(
        config.model_prices.iter().all(|row| {
            !(row.provider_id.as_deref() == Some("p") && row.upstream_model == "unreachable")
        }),
        "a row unreachable from its provider must be dropped"
    );

    let p_rows: Vec<&ModelPrice> = config
        .model_prices
        .iter()
        .filter(|row| row.provider_id.as_deref() == Some("p"))
        .collect();
    assert_eq!(
        p_rows.len(),
        2,
        "P must hold remote-a and the migrated remote-default: {p_rows:?}"
    );
    let p_remote_a = p_rows
        .iter()
        .find(|row| row.upstream_model == "remote-a")
        .expect("P keeps its scoped remote-a row");
    assert_eq!(
        p_remote_a.input, 9.0,
        "the pre-existing scoped row must not be overwritten or duplicated"
    );
    let p_default = p_rows
        .iter()
        .find(|row| row.upstream_model == "remote-default")
        .expect("the global default-model row must migrate to P");
    assert_eq!(p_default.input, 2.0);

    let q_rows: Vec<&ModelPrice> = config
        .model_prices
        .iter()
        .filter(|row| row.provider_id.as_deref() == Some("q"))
        .collect();
    assert_eq!(
        q_rows.len(),
        1,
        "Q's duplicate rows must collapse to one: {q_rows:?}"
    );
    let q_row = q_rows[0];
    assert_eq!(q_row.upstream_model, "remote-q");
    assert_eq!(q_row.input, 4.0, "deduplication keeps the first row");
}

/// The in-memory migration result also carries the singular `off_peak` window
/// forward as the first `off_peaks` entry.
fn assert_migrated_in_memory(config: &crate::ai_gateway::GatewayConfig) {
    assert_price_scoping(config);
    let q_row = config
        .model_prices
        .iter()
        .find(|row| row.provider_id.as_deref() == Some("q"))
        .expect("Q's scoped row");
    assert_eq!(
        q_row.off_peaks.len(),
        1,
        "a singular off_peak window must become the first off_peaks entry"
    );
    assert_eq!(q_row.off_peaks[0].start_time, "00:30");
    assert_eq!(q_row.off_peaks[0].end_time, "08:30");
    assert_eq!(q_row.off_peaks[0].days, Some(vec![1, 2, 3]));
}

fn assert_on_disk_migrated(on_disk: &Value) {
    let version = on_disk
        .get("schema_version")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("the migrated config must carry a schema_version: {on_disk}"));
    assert!(version >= 1, "the schema version must be a positive number");

    let rows = on_disk["model_prices"]
        .as_array()
        .unwrap_or_else(|| panic!("model_prices must be an array: {on_disk}"));
    assert!(!rows.is_empty(), "the migrated config must keep its price rows");
    for row in rows {
        assert!(
            row["provider_id"].as_str().is_some(),
            "every persisted row must be provider-scoped: {row}"
        );
        assert!(
            row.get("off_peak").is_none(),
            "the singular off_peak key must be gone: {row}"
        );
    }
    let q_row = rows
        .iter()
        .find(|row| row["provider_id"] == "q")
        .expect("Q's scoped row must be persisted");
    assert_eq!(
        q_row["off_peaks"].as_array().map(Vec::len),
        Some(1),
        "the singular window must be persisted as off_peaks: {q_row}"
    );

    for provider in on_disk["providers"].as_array().expect("providers array") {
        for key in PROVIDER_RUNTIME_KEYS {
            assert!(
                provider.get(key).is_none(),
                "provider-level key {key} must be gone: {provider}"
            );
        }
    }
}

/// AC-004: the in-memory value after reading a legacy configuration equals the
/// former per-read normalization result (provider-scoped rows, dedup, singular
/// window converted into `off_peaks`).
#[test]
fn legacy_config_read_migrates_in_memory_values() {
    let _home = isolated_temp_home("migration-legacy-in-memory");
    write_encrypted_config(&legacy_config_fixture());

    let migrated = read_config().expect("legacy config must stay readable");
    assert_migrated_in_memory(&migrated);

    let second = read_config().expect("second read");
    assert_eq!(
        serde_json::to_value(&second).expect("second config"),
        serde_json::to_value(&migrated).expect("first config"),
        "a second read must be value-equivalent"
    );
}

/// AC-001 / AC-002: the first read of a legacy configuration atomically
/// rewrites the file at the current schema version with provider-scoped
/// deduplicated rows, `off_peaks` and no singular field; a second read writes
/// nothing.
#[test]
fn legacy_config_read_persists_current_version() {
    let _home = isolated_temp_home("migration-legacy-persist");
    write_encrypted_config(&legacy_config_fixture());
    let legacy_bytes = raw_config_bytes();

    let _migrated = read_config().expect("legacy config must stay readable");

    let bytes_after_first_read = raw_config_bytes();
    assert_ne!(
        bytes_after_first_read, legacy_bytes,
        "the first read of a legacy config must persist the migrated bytes"
    );
    assert_on_disk_migrated(&decrypted_config_json());

    let stable_bytes = raw_config_bytes();
    let _second = read_config().expect("second read");
    assert_eq!(
        raw_config_bytes(),
        stable_bytes,
        "a second read must not write again"
    );
}

/// AC-008 / REQ-004: the rewritten on-disk configuration no longer carries any
/// provider-level runtime key.
#[test]
fn legacy_config_read_drops_provider_runtime_keys() {
    let _home = isolated_temp_home("migration-legacy-provider-keys");
    write_encrypted_config(&legacy_config_fixture());

    let _migrated = read_config().expect("legacy config must stay readable");

    let on_disk = decrypted_config_json();
    for provider in on_disk["providers"].as_array().expect("providers array") {
        for key in PROVIDER_RUNTIME_KEYS {
            assert!(
                provider.get(key).is_none(),
                "provider-level key {key} must be gone after the rewrite: {provider}"
            );
        }
    }
}

/// AC-002: a configuration already at the current schema version is read with
/// no migration write and unchanged bytes.
#[test]
fn current_version_config_read_is_side_effect_free() {
    let _home = isolated_temp_home("migration-current-version");
    let current = json!({
        "schema_version": crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION,
        "enabled": true,
        "providers": [{
            "id": "p",
            "name": "Provider P",
            "base_url": "https://p.example.com/v1",
            "api_key": "sk-p",
            "protocol": "chat_completions",
            "mappings": [
                {"local_model": "local-a", "upstream_model": "remote-a", "enabled": true}
            ]
        }],
        "model_prices": [
            {"provider_id": "p", "upstream_model": "remote-a", "input": 1.0}
        ]
    });
    write_encrypted_config(&current);
    let before = raw_config_bytes();

    let loaded = read_config().expect("current config must stay readable");
    assert_eq!(loaded.providers.len(), 1);
    assert_eq!(loaded.model_prices.len(), 1);
    assert_eq!(
        raw_config_bytes(),
        before,
        "reading a current-version config must not write"
    );
}

/// A read-only permissions guard that restores write access before the temp
/// home is removed, even when an assertion panics.
#[cfg(unix)]
struct RestoreDirPermissions(PathBuf);

#[cfg(unix)]
impl Drop for RestoreDirPermissions {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700));
    }
}

/// AC-003: when the configuration rewrite cannot complete (temporarily
/// read-only directory) the read still returns the migrated value, the on-disk
/// file keeps its previous bytes and a later read completes the migration.
#[cfg(unix)]
#[test]
fn failed_rewrite_is_retried_and_completed_by_a_later_read() {
    use std::os::unix::fs::PermissionsExt;

    let _home = isolated_temp_home("migration-readonly-retry");
    write_encrypted_config(&legacy_config_fixture());
    let legacy_bytes = raw_config_bytes();

    // The rewrite writes `ai_gateway.tmp` inside the application directory, not
    // the temp-home root: making the ancestor read-only would not block it.
    let dir = crate::config::get_app_dir().expect("app dir");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o500))
        .expect("make the app directory read-only");
    let _restore = RestoreDirPermissions(dir.clone());

    let migrated = read_config().expect("the read must still return the migrated value");
    assert_price_scoping(&migrated);
    assert_eq!(
        raw_config_bytes(),
        legacy_bytes,
        "a failed rewrite must leave the previous complete bytes on disk"
    );

    // Restore write access; the next read must complete the migration.
    drop(_restore);
    let completed = read_config().expect("the retry read must succeed");
    assert_price_scoping(&completed);
    let on_disk = decrypted_config_json();
    assert!(
        on_disk.get("schema_version").and_then(Value::as_u64).is_some(),
        "the retried read must persist the migrated config: {on_disk}"
    );
    assert_on_disk_migrated(&on_disk);
}

/// An encrypted configuration without a schema version whose data is already in
/// the current shape: a provider with an enabled mapping and a provider-scoped
/// price row carrying a non-empty `off_peaks` list, with no singular `off_peak`
/// key and no provider-level runtime keys. Nothing here needs a transformation,
/// yet the read must still advance the version checkpoint once.
fn versionless_current_shape_fixture() -> Value {
    json!({
        "enabled": true,
        "providers": [
            {
                "id": "p",
                "name": "Provider P",
                "base_url": "https://p.example.com/v1",
                "api_key": "sk-p",
                "protocol": "chat_completions",
                "mappings": [
                    {"local_model": "local-a", "upstream_model": "remote-a", "enabled": true}
                ]
            }
        ],
        "model_prices": [
            {
                "provider_id": "p",
                "upstream_model": "remote-a",
                "input": 1.5,
                "cache_read": 0.25,
                "cache_write": 0.5,
                "output": 3.0,
                "off_peaks": [
                    {
                        "start_time": "00:00",
                        "end_time": "08:00",
                        "input": 0.75,
                        "cache_read": 0.1,
                        "cache_write": 0.2,
                        "output": 1.5,
                        "days": [1, 2, 3]
                    }
                ]
            }
        ]
    })
}

/// Boundary: a configuration without a schema version whose data is already in
/// the current shape migrates idempotently and changes no value. The first read
/// must stamp the current schema version on disk even though the migration had
/// nothing to rewrite, and the second read must write nothing.
#[test]
fn versionless_config_already_in_new_shape_is_stamped_once() {
    let _home = isolated_temp_home("migration-versionless-current-shape");
    let fixture = versionless_current_shape_fixture();
    write_encrypted_config(&fixture);

    let loaded = read_config().expect("the version-less config must stay readable");

    // The first read must persist the current schema version even though every
    // value already matches the new shape.
    let first_on_disk = decrypted_config_json();
    assert_eq!(
        first_on_disk.get("schema_version").and_then(Value::as_u64),
        Some(u64::from(crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION)),
        "the first read must stamp the version-less configuration: {first_on_disk}"
    );

    // In-memory value: the current version with unchanged providers and prices.
    assert_eq!(
        loaded.schema_version,
        crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION,
        "the in-memory config must carry the current schema version"
    );
    let loaded_ids: Vec<&str> = loaded.providers.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(loaded_ids, vec!["p"], "the provider set must be unchanged");
    assert!(
        loaded.providers[0].mappings[0].enabled,
        "the enabled mapping must be preserved"
    );
    let expected_prices: Vec<ModelPrice> =
        serde_json::from_value(fixture["model_prices"].clone()).expect("expected price rows");
    assert_eq!(
        loaded.model_prices, expected_prices,
        "the price rows must be unchanged by the stamping read"
    );

    // On-disk value: only the version checkpoint advanced; provider ids and
    // every price row (including `off_peaks`) are untouched.
    let fixture_ids: Vec<&str> = fixture["providers"]
        .as_array()
        .expect("fixture providers")
        .iter()
        .map(|provider| provider["id"].as_str().expect("fixture provider id"))
        .collect();
    let on_disk_ids: Vec<&str> = first_on_disk["providers"]
        .as_array()
        .expect("on-disk providers")
        .iter()
        .map(|provider| provider["id"].as_str().expect("on-disk provider id"))
        .collect();
    assert_eq!(on_disk_ids, fixture_ids, "the provider ids must be preserved");
    assert_eq!(
        first_on_disk["model_prices"], fixture["model_prices"],
        "the persisted price rows must be value-identical: {first_on_disk}"
    );
    for row in first_on_disk["model_prices"].as_array().expect("on-disk rows") {
        assert!(
            row.get("off_peak").is_none(),
            "no singular off_peak key may be introduced: {row}"
        );
        assert_eq!(
            row["off_peaks"].as_array().map(Vec::len),
            Some(1),
            "the off_peaks list must be preserved: {row}"
        );
    }

    // Idempotent: the second read writes nothing.
    let stable_bytes = raw_config_bytes();
    let _second = read_config().expect("the second read must succeed");
    assert_eq!(
        raw_config_bytes(),
        stable_bytes,
        "a second read must not write again"
    );
}

/// Counterexample (REQ-001 / AC-004): a single global price row whose upstream
/// model is reachable by two providers must be copied to BOTH providers — never
/// dropped, never assigned to only one — and the global row must disappear. The
/// two providers reach the same model through different rules: P through a
/// mapping, Q through its `default_model`. The expected values are independent
/// of each other so an "assigned to one provider only" regression fails both the
/// count and the per-provider assertions.
#[test]
fn legacy_global_row_reachable_by_two_providers_is_copied_to_both() {
    let _home = isolated_temp_home("migration-multi-provider-copy");
    let fixture = json!({
        "enabled": true,
        "providers": [
            {
                "id": "p",
                "name": "Provider P",
                "base_url": "https://p.example.com/v1",
                "api_key": "sk-p",
                "protocol": "chat_completions",
                "mappings": [
                    {"local_model": "local-shared", "upstream_model": "shared-model", "enabled": true}
                ]
            },
            {
                "id": "q",
                "name": "Provider Q",
                "base_url": "https://q.example.com/v1",
                "api_key": "sk-q",
                "protocol": "chat_completions",
                "default_model": "shared-model",
                "mappings": [
                    {"local_model": "local-other", "upstream_model": "other-model", "enabled": true}
                ]
            }
        ],
        "model_prices": [
            {"upstream_model": "shared-model", "input": 1.25, "output": 2.5}
        ]
    });
    write_encrypted_config(&fixture);

    let migrated = read_config().expect("the legacy config must stay readable");

    assert_eq!(
        migrated.model_prices.len(),
        2,
        "the global row must become exactly one scoped row per reaching provider: {:?}",
        migrated.model_prices
    );
    assert!(
        migrated
            .model_prices
            .iter()
            .all(|row| row.provider_id.is_some()),
        "the global row must be gone and every row provider-scoped: {:?}",
        migrated.model_prices
    );
    for provider_id in ["p", "q"] {
        let row = migrated
            .model_prices
            .iter()
            .find(|row| row.provider_id.as_deref() == Some(provider_id))
            .unwrap_or_else(|| panic!("provider {provider_id} must receive the migrated row"));
        assert_eq!(row.upstream_model, "shared-model");
        assert_eq!(row.input, 1.25, "the copied row keeps its input tier");
        assert_eq!(row.output, 2.5, "the copied row keeps its output tier");
    }

    // The rewrite must persist both scoped rows at the current version and no
    // global row.
    let on_disk = decrypted_config_json();
    assert_eq!(
        on_disk.get("schema_version").and_then(Value::as_u64),
        Some(u64::from(crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION)),
        "the migrated config must be persisted at the current version: {on_disk}"
    );
    let rows = on_disk["model_prices"]
        .as_array()
        .unwrap_or_else(|| panic!("on-disk model_prices must be an array: {on_disk}"));
    assert_eq!(rows.len(), 2, "both scoped rows must be persisted: {on_disk}");
    for row in rows {
        let provider_id = row["provider_id"]
            .as_str()
            .unwrap_or_else(|| panic!("every on-disk row must be provider-scoped: {row}"));
        assert!(
            provider_id == "p" || provider_id == "q",
            "unexpected provider id {provider_id}: {row}"
        );
        assert_eq!(row["upstream_model"], "shared-model");
        assert_eq!(row["input"], 1.25);
        assert_eq!(row["output"], 2.5);
    }
}

/// Counterexample (REQ-001 / AC-004): a legacy price row carrying an explicit
/// JSON `null` provider id must deserialize and migrate as a global row; the
/// whole parse must never fail. The reachable null row is scoped to its provider
/// and the unreachable null row is dropped by the normal reachability rule. A
/// regression that narrowed null-tolerance, or mistook `null` for a scoped
/// provider, would either error the read or leave P without its row.
#[test]
fn legacy_null_provider_id_deserializes_and_migrates_as_global() {
    let _home = isolated_temp_home("migration-null-provider-id");
    let fixture = json!({
        "enabled": true,
        "providers": [
            {
                "id": "p",
                "name": "Provider P",
                "base_url": "https://p.example.com/v1",
                "api_key": "sk-p",
                "protocol": "chat_completions",
                "mappings": [
                    {"local_model": "local-null", "upstream_model": "remote-null", "enabled": true}
                ]
            }
        ],
        "model_prices": [
            {"provider_id": null, "upstream_model": "remote-null", "input": 3.0},
            {"provider_id": null, "upstream_model": "remote-orphan", "input": 7.0}
        ]
    });
    write_encrypted_config(&fixture);

    let migrated = read_config().expect("a null provider_id must never fail the whole parse");

    assert_eq!(
        migrated.model_prices.len(),
        1,
        "only the reachable null row survives, scoped to P: {:?}",
        migrated.model_prices
    );
    let row = &migrated.model_prices[0];
    assert_eq!(row.provider_id.as_deref(), Some("p"));
    assert_eq!(row.upstream_model, "remote-null");
    assert_eq!(row.input, 3.0);

    // The reachable null row is persisted scoped; the unreachable one is gone.
    let on_disk = decrypted_config_json();
    let rows = on_disk["model_prices"]
        .as_array()
        .unwrap_or_else(|| panic!("on-disk model_prices must be an array: {on_disk}"));
    assert_eq!(rows.len(), 1, "only the scoped row may persist: {on_disk}");
    assert_eq!(rows[0]["provider_id"], "p");
    assert_eq!(rows[0]["upstream_model"], "remote-null");
    assert_eq!(rows[0]["input"], 3.0);
}

/// Boundary: a configuration at a future/unknown schema version (here `99`) is
/// read as-is. There is no downgrade, no rewrite and no deletion: values are
/// preserved in memory — including a global price row the current migration
/// would otherwise scope — and the raw file bytes stay byte-identical after the
/// read. A regression that treated any non-current version as migratable would
/// rewrite the bytes and scope the row.
#[test]
fn future_schema_version_config_is_read_without_downgrade_or_rewrite() {
    let _home = isolated_temp_home("migration-future-version");
    let fixture = json!({
        "schema_version": 99,
        "enabled": true,
        "providers": [
            {
                "id": "p",
                "name": "Provider P",
                "base_url": "https://p.example.com/v1",
                "api_key": "sk-p",
                "protocol": "chat_completions",
                "mappings": [
                    {"local_model": "local-a", "upstream_model": "remote-a", "enabled": true}
                ]
            }
        ],
        "model_prices": [
            {"upstream_model": "remote-a", "input": 4.0}
        ],
        "future_only_field": {"kept": true}
    });
    write_encrypted_config(&fixture);
    let before = raw_config_bytes();

    let loaded = read_config().expect("a future-version config must stay readable");

    assert_eq!(
        loaded.schema_version, 99,
        "the future version must be preserved, not downgraded"
    );
    assert_eq!(loaded.providers.len(), 1, "providers must be preserved");
    assert_eq!(
        loaded.model_prices.len(),
        1,
        "the global price row must be preserved untouched"
    );
    assert_eq!(
        loaded.model_prices[0].provider_id, None,
        "no migration may scope a future-version config's rows"
    );
    assert_eq!(loaded.model_prices[0].upstream_model, "remote-a");
    assert_eq!(loaded.model_prices[0].input, 4.0);

    assert_eq!(
        raw_config_bytes(),
        before,
        "reading a future-version config must not rewrite or delete anything"
    );
}

// ---------------------------------------------------------------------------
// Usage-database migration
// ---------------------------------------------------------------------------

fn create_prev_version_usage_db(path: &Path) {
    let connection = Connection::open(path).expect("open raw usage db");
    connection
        .execute_batch(
            "CREATE TABLE usage_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp_ms INTEGER NOT NULL,
                local_model TEXT NOT NULL,
                upstream_model TEXT NOT NULL,
                provider_id TEXT NOT NULL,
                provider_name TEXT NOT NULL,
                result TEXT NOT NULL,
                status INTEGER NOT NULL,
                input_tokens INTEGER NOT NULL,
                cache_read_tokens INTEGER NOT NULL,
                cache_write_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                amount REAL,
                duration_ms INTEGER NOT NULL,
                error_message TEXT,
                terminal INTEGER NOT NULL DEFAULT 1,
                reasoning_effort TEXT,
                usage_semantics TEXT NOT NULL DEFAULT 'legacy',
                usage_present INTEGER NOT NULL DEFAULT 0,
                cache_accounting_valid INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX idx_usage_logs_timestamp ON usage_logs(timestamp_ms);
            CREATE INDEX idx_usage_logs_local_model ON usage_logs(local_model);",
        )
        .expect("create raw usage schema");
}

fn insert_raw_usage_row(
    path: &Path,
    timestamp_ms: i64,
    local_model: &str,
    upstream_model: &str,
    provider_id: &str,
    provider_name: &str,
    result: &str,
    amount: Option<f64>,
) {
    let connection = Connection::open(path).expect("open raw usage db");
    connection
        .execute(
            "INSERT INTO usage_logs (
                timestamp_ms, local_model, upstream_model, provider_id, provider_name,
                result, status, input_tokens, cache_read_tokens, cache_write_tokens,
                output_tokens, total_tokens, amount, duration_ms, error_message, terminal,
                reasoning_effort, usage_semantics, usage_present, cache_accounting_valid
            ) VALUES (?, ?, ?, ?, ?, ?, 200, 10, 0, 0, 5, 15, ?, 1, NULL, 1, NULL, 'canonical_v1', 1, 1)",
            rusqlite::params![
                timestamp_ms,
                local_model,
                upstream_model,
                provider_id,
                provider_name,
                result,
                amount,
            ],
        )
        .expect("insert raw usage row");
}

fn count_cancelled(path: &Path) -> i64 {
    let connection = Connection::open(path).expect("open raw usage db");
    connection
        .query_row(
            "SELECT COUNT(*) FROM usage_logs WHERE result = 'cancelled'",
            [],
            |row| row.get(0),
        )
        .expect("count cancelled rows")
}

/// AC-005 / REQ-002: the first open physically deletes cancelled rows while the
/// visible statistics and log pages keep their values; a second open deletes
/// nothing.
#[test]
fn usage_store_first_open_deletes_legacy_cancelled_once() {
    let dir = std::env::temp_dir().join(format!(
        "onespace-ai-gateway-migration-usage-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("ai_gateway_usage.db");
    create_prev_version_usage_db(&path);
    insert_raw_usage_row(&path, 1_000, "local-a", "remote-a", "p1", "Provider One", "success", Some(0.5));
    insert_raw_usage_row(&path, 2_000, "local-b", "remote-b", "p2", "Provider Two", "failure", None);
    insert_raw_usage_row(&path, 3_000, "local-cancelled", "remote-c", "p1", "Provider One", "cancelled", None);
    assert_eq!(count_cancelled(&path), 1, "the fixture must carry a cancelled row");

    let store = UsageLogStore::at(path.clone());

    // First open (triggered by a query): visible results are unchanged.
    let stats = store
        .usage_stats(&TimeRange::default(), false)
        .expect("usage stats");
    assert_eq!(stats.totals.request_count, 2, "success + failure stay visible");
    assert_eq!(stats.totals.total_tokens, 30, "the visible token total is unchanged");
    let page = store
        .query_logs(&TimeRange::default(), &LogFilter::default(), 1)
        .expect("request logs");
    assert_eq!(page.total, 2, "the visible request-log page is unchanged");
    assert_eq!(page.records.len(), 2);
    assert!(
        !page.models.iter().any(|model| model == "local-cancelled"),
        "a cancelled row must not surface in the model facet: {:?}",
        page.models
    );
    assert_eq!(
        count_cancelled(&path),
        0,
        "the first open must physically delete every cancelled row"
    );

    // A row inserted after the migration marker advanced must survive the next
    // open: later opens delete nothing.
    insert_raw_usage_row(&path, 4_000, "local-late-cancelled", "remote-c", "p1", "Provider One", "cancelled", None);
    assert_eq!(count_cancelled(&path), 1);
    let _ = store
        .usage_stats(&TimeRange::default(), false)
        .expect("second open usage stats");
    assert_eq!(
        count_cancelled(&path),
        1,
        "a second open must not delete cancelled rows again"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// AC-004/AC-005: a query connection bound to a path must reopen a replaced
/// legacy database and run the one-time version migration on that actual file.
/// The fresh file's visible rows replace the old file's rows, its own cancelled
/// row is removed exactly once, and `user_version` advances to 2. A later open
/// must not delete newly inserted cancelled rows again.
#[test]
fn replaced_legacy_usage_database_is_reopened_and_migrated_by_queries() {
    let dir = make_temp_dir("migration-query-reopen");
    fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("ai_gateway_usage.db");
    create_prev_version_usage_db(&path);
    insert_raw_usage_row(
        &path,
        1_000,
        "old-model",
        "old-upstream",
        "p-old",
        "Old Provider",
        "success",
        Some(0.5),
    );
    insert_raw_usage_row(
        &path,
        2_000,
        "old-cancelled",
        "old-upstream",
        "p-old",
        "Old Provider",
        "cancelled",
        None,
    );

    let store = UsageLogStore::at(path.clone());
    let first = store
        .usage_stats(&TimeRange::default(), false)
        .expect("first query migrates the original database");
    assert_eq!(first.totals.request_count, 1);
    assert_eq!(count_cancelled(&path), 0);

    let replacement_dir = make_temp_dir("migration-query-reopen-replacement");
    fs::create_dir_all(&replacement_dir).expect("create replacement dir");
    let replacement_path = replacement_dir.join("ai_gateway_usage.db");
    create_prev_version_usage_db(&replacement_path);
    insert_raw_usage_row(
        &replacement_path,
        3_000,
        "new-model",
        "new-upstream",
        "p-new",
        "New Provider",
        "success",
        Some(0.75),
    );
    insert_raw_usage_row(
        &replacement_path,
        4_000,
        "new-cancelled",
        "new-upstream",
        "p-new",
        "New Provider",
        "cancelled",
        None,
    );

    fs::rename(&replacement_path, &path).expect("replace the usage database");
    let second = store
        .usage_stats(&TimeRange::default(), false)
        .expect("query after replacement");
    assert_eq!(second.totals.request_count, 1, "old rows must not survive");
    assert_eq!(second.totals.total_tokens, 15, "the replacement rows must be read");
    let page = store
        .query_logs(&TimeRange::default(), &LogFilter::default(), 1)
        .expect("log page after replacement");
    assert_eq!(page.total, 1);
    assert_eq!(page.records[0].local_model, "new-model");
    assert_eq!(count_cancelled(&path), 0, "the replacement must be migrated");

    let version: i64 = Connection::open(&path)
        .expect("open replaced database")
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read user_version");
    assert_eq!(
        version, USAGE_DB_VERSION as i64,
        "the replaced database must reach the current usage schema"
    );

    // The marker is already current for this file: a later cancelled row is not
    // removed by a subsequent query.
    insert_raw_usage_row(
        &path,
        5_000,
        "late-cancelled",
        "new-upstream",
        "p-new",
        "New Provider",
        "cancelled",
        None,
    );
    let _ = store
        .usage_stats(&TimeRange::default(), false)
        .expect("query after the one-time migration");
    assert_eq!(
        count_cancelled(&path),
        1,
        "a later open must not repeat the version-0 cancellation cleanup"
    );

    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&replacement_dir);
}

// ---------------------------------------------------------------------------
// Ordered named key pool migration (REQ-002 / AC-002)
//
// The key-pool fields are not on the typed provider yet, so the migrated shape
// is asserted on the decrypted raw JSON. The fixtures carry the legacy
// `api_key` field; the expected post-migration shape is schema version 2 with a
// `keys` array and no `api_key`.
// ---------------------------------------------------------------------------

/// Encoded current key-pool schema version.
const KEY_POOL_SCHEMA_VERSION: u32 = crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION;

/// One migrated `Default` key entry for `value`.
fn expected_default_key(value: &str) -> Value {
    json!({
        "id": "default",
        "name": "Default",
        "value": value,
        "enabled": true,
        "auto_marked": false,
        "failure_kind": null,
        "marked_at": null,
        "reason": null,
    })
}

/// A version-less configuration whose first provider holds a legacy single
/// credential and whose second provider holds a blank one.
fn legacy_single_key_fixture() -> Value {
    json!({
        "enabled": true,
        "providers": [
            {
                "id": "p",
                "name": "Provider P",
                "base_url": "https://p.example.com/v1",
                "api_key": "sk-legacy-secret",
                "protocol": "chat_completions",
                "mappings": [
                    {"local_model": "local-a", "upstream_model": "remote-a", "enabled": true}
                ]
            },
            {
                "id": "q",
                "name": "Provider Q",
                "base_url": "https://q.example.com/v1",
                "api_key": "   ",
                "protocol": "chat_completions",
                "mappings": [
                    {"local_model": "local-q", "upstream_model": "remote-q", "enabled": true}
                ]
            }
        ]
    })
}

/// AC-002 / REQ-002: the first read of a legacy single-key configuration
/// converts it once into a one-entry pool named `Default`, removes the old
/// credential field, stamps schema version 2 and writes atomically; the second
/// read leaves the bytes identical and a blank legacy value yields an empty
/// pool.
#[test]
fn legacy_single_key_migrates_to_default_named_pool_once() {
    let _home = isolated_temp_home("keypool-legacy-single");
    write_encrypted_config(&legacy_single_key_fixture());

    let first = read_config().expect("the legacy key configuration must stay readable");
    assert_eq!(
        first.schema_version, KEY_POOL_SCHEMA_VERSION,
        "the in-memory config must carry the key-pool schema version"
    );

    let on_disk = decrypted_config_json();
    assert_eq!(
        on_disk.get("schema_version").and_then(Value::as_u64),
        Some(u64::from(KEY_POOL_SCHEMA_VERSION)),
        "the rewrite must persist schema version 2: {on_disk}"
    );
    let providers = on_disk["providers"].as_array().expect("providers array");
    let p = providers
        .iter()
        .find(|provider| provider["id"] == "p")
        .expect("provider p must be persisted");
    assert!(
        p.get("api_key").is_none(),
        "the old single credential field must be removed: {p}"
    );
    let keys = p["keys"]
        .as_array()
        .unwrap_or_else(|| panic!("provider p must carry a migrated keys array: {p}"));
    assert_eq!(keys.len(), 1, "the legacy key must become exactly one entry");
    assert_eq!(keys[0]["name"], "Default");
    assert_eq!(keys[0]["value"], "sk-legacy-secret");
    assert_eq!(keys[0]["enabled"], true);
    assert!(
        keys[0]["id"].as_str().is_some_and(|id| !id.trim().is_empty()),
        "the migrated Default key must carry a stable non-empty id: {keys:?}"
    );
    assert_eq!(
        keys[0]["auto_marked"], false,
        "a migrated legacy key starts healthy: {keys:?}"
    );

    let q = providers
        .iter()
        .find(|provider| provider["id"] == "q")
        .expect("provider q must be persisted");
    assert!(q.get("api_key").is_none());
    assert_eq!(
        q["keys"].as_array().map(Vec::len),
        Some(0),
        "a blank legacy value must yield an empty pool: {q}"
    );

    let raw = fs::read(config_path().expect("config path")).expect("read raw config");
    assert!(
        !String::from_utf8_lossy(&raw).contains("sk-legacy-secret"),
        "the migrated key value must never be stored in plaintext"
    );

    let stable = raw_config_bytes();
    let _second = read_config().expect("the second read must succeed");
    assert_eq!(
        raw_config_bytes(),
        stable,
        "the second read of an already-migrated config must not write again"
    );
}

/// AC-002 / REQ-002: a current-version configuration that already carries a key
/// pool is read without any rewrite; the keys survive untouched.
#[test]
fn current_version_key_pool_file_is_not_rewritten() {
    let _home = isolated_temp_home("keypool-current-version");
    let fixture = json!({
        "schema_version": KEY_POOL_SCHEMA_VERSION,
        "enabled": true,
        "providers": [{
            "id": "p",
            "name": "Provider P",
            "base_url": "https://p.example.com/v1",
            "protocol": "chat_completions",
            "keys": [
                expected_default_key("sk-current-secret"),
                {
                    "id": "key-b",
                    "name": "B",
                    "value": "sk-b",
                    "enabled": false,
                    "auto_marked": false,
                    "failure_kind": null,
                    "marked_at": null,
                    "reason": null,
                }
            ],
            "mappings": [
                {"local_model": "local-a", "upstream_model": "remote-a", "enabled": true}
            ]
        }]
    });
    write_encrypted_config(&fixture);
    let before = raw_config_bytes();

    let loaded = read_config().expect("the current-version pool must stay readable");
    assert_eq!(loaded.schema_version, KEY_POOL_SCHEMA_VERSION);
    assert_eq!(
        raw_config_bytes(),
        before,
        "reading a current-version key-pool file must not write"
    );

    let on_disk = decrypted_config_json();
    let keys = on_disk["providers"][0]["keys"]
        .as_array()
        .unwrap_or_else(|| panic!("the keys array must be preserved: {on_disk}"));
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0]["value"], "sk-current-secret");
    assert_eq!(keys[1]["enabled"], false);
}

/// AC-002 / REQ-002 counterexample: a future/unknown version is read as-is
/// without a downgrade, a rewrite or any field deletion.
#[test]
fn future_version_key_pool_file_is_left_untouched() {
    let _home = isolated_temp_home("keypool-future-version");
    let fixture = json!({
        "schema_version": 99,
        "enabled": true,
        "providers": [{
            "id": "p",
            "name": "Provider P",
            "base_url": "https://p.example.com/v1",
            "api_key": "sk-legacy-untouched",
            "protocol": "chat_completions",
            "mappings": []
        }],
        "future_only_field": {"kept": true}
    });
    write_encrypted_config(&fixture);
    let before = raw_config_bytes();

    let loaded = read_config().expect("a future-version config must stay readable");
    assert_eq!(loaded.schema_version, 99, "the future version must be preserved");
    assert_eq!(
        raw_config_bytes(),
        before,
        "a future-version file must never be rewritten or downgraded"
    );

    let on_disk = decrypted_config_json();
    assert_eq!(on_disk["future_only_field"], json!({"kept": true}));
    assert_eq!(
        on_disk["providers"][0]["api_key"], "sk-legacy-untouched",
        "no future-version field may be mutated"
    );
}

/// AC-002: when the migration rewrite cannot complete the read still returns
/// the migrated value, the on-disk file keeps its previous complete bytes and a
/// later read completes the same migration.
#[cfg(unix)]
#[test]
fn failed_key_pool_rewrite_keeps_previous_bytes_and_retries() {
    use std::os::unix::fs::PermissionsExt;

    let _home = isolated_temp_home("keypool-readonly-retry");
    write_encrypted_config(&legacy_single_key_fixture());
    let legacy_bytes = raw_config_bytes();

    let dir = crate::config::get_app_dir().expect("app dir");
    // The rewrite writes `ai_gateway.tmp` inside the application directory.
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o500))
        .expect("make the app directory read-only");
    let _restore = RestoreDirPermissions(dir.clone());

    let migrated = read_config().expect("the read must still return the migrated value");
    assert_eq!(
        migrated.schema_version, KEY_POOL_SCHEMA_VERSION,
        "the in-memory value must be migrated even when the rewrite fails"
    );
    assert_eq!(
        raw_config_bytes(),
        legacy_bytes,
        "a failed rewrite must keep the previous complete bytes"
    );

    drop(_restore);
    let completed = read_config().expect("the retry read must succeed");
    assert_eq!(completed.schema_version, KEY_POOL_SCHEMA_VERSION);

    let on_disk = decrypted_config_json();
    assert_eq!(
        on_disk.get("schema_version").and_then(Value::as_u64),
        Some(u64::from(KEY_POOL_SCHEMA_VERSION)),
        "the retried read must persist the migration: {on_disk}"
    );
    let provider = &on_disk["providers"][0];
    assert!(provider.get("api_key").is_none(), "the old field must be gone: {provider}");
    let keys = provider["keys"]
        .as_array()
        .unwrap_or_else(|| panic!("the retried migration must persist keys: {provider}"));
    assert_eq!(keys[0]["name"], "Default");
    assert_eq!(keys[0]["value"], "sk-legacy-secret");
}

// ===========================================================================
// 20261010-provider-template-bound-strategies — REQ-007 (AC-018..AC-022):
// the version-gated v2-to-v3 OpenCode Zen -> OpenCode Go migration.
//
// Fixtures are raw JSON so this module keeps compiling against the current API
// and fails behaviorally. Go baseline ids are read from the shipped built-in at
// runtime, so the fixtures never hard-code a model list.
// ===========================================================================

/// The schema version the Zen-to-Go migration advances to.
const ZEN_TO_GO_SCHEMA_VERSION: u32 = crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION;

/// The schema version the ordered fallback model list advances to. The
/// legacy single provider-specific fallback migrates into a one-entry
/// gateway-model list under this version gate.
const ORDERED_FALLBACK_SCHEMA_VERSION: u32 = crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION;

/// One declared Chat Completions and one declared Responses id from the shipped
/// Go baseline. Reading them at runtime keeps the fixtures independent of the
/// exact documented model ids.
struct GoBaselineIds {
    chat: String,
    responses: String,
}

fn go_baseline_ids() -> GoBaselineIds {
    let go = crate::ai_gateway::templates::find_builtin_template("opencode-go")
        .expect("the OpenCode Go built-in must exist");
    let chat = go
        .models
        .iter()
        .find(|model| model.protocol == Some(crate::ai_gateway::UpstreamProtocol::ChatCompletions))
        .expect("the Go baseline must declare a Chat Completions model")
        .upstream_model
        .clone();
    let responses = go
        .models
        .iter()
        .find(|model| model.protocol == Some(crate::ai_gateway::UpstreamProtocol::Responses))
        .expect("the Go baseline must declare a Responses model")
        .upstream_model
        .clone();
    GoBaselineIds { chat, responses }
}

const LEGACY_IMPLICIT_MODEL: &str = "legacy-model-not-in-go-baseline";
const LEGACY_EXPLICIT_MODEL: &str = "legacy-explicit-not-in-go-baseline";

/// A schema-2 configuration with an `opencode-zen` template snapshot, one bound
/// provider carrying account/user fields, legacy and supported mappings, price
/// rows and an unsupported default model, and one URL-only unbound provider.
fn zen_schema2_fixture(ids: &GoBaselineIds) -> Value {
    let chat = ids.chat.as_str();
    let responses = ids.responses.as_str();
    json!({
        "schema_version": 2,
        "enabled": true,
        "providers": [
            {
                "id": "zen-bound",
                "name": "My Zen",
                "base_url": "https://opencode.ai/zen/v1",
                "keys": [super::pool_key("key-default", "Default", "sk-zen-secret", true)],
                "default_model": "legacy-zen-default",
                "protocol": "chat_completions",
                "template_id": "opencode-zen",
                "weight": 7,
                "enabled": true,
                "tags": ["prod"],
                "icon": "custom-icon",
                "mappings": [
                    {"local_model": chat, "upstream_model": chat, "enabled": true},
                    {"local_model": "legacy-implicit", "upstream_model": LEGACY_IMPLICIT_MODEL, "enabled": true},
                    {"local_model": responses, "upstream_model": responses, "enabled": true},
                    {"local_model": "legacy-explicit", "upstream_model": LEGACY_EXPLICIT_MODEL, "enabled": true, "protocol": "responses"}
                ]
            },
            {
                "id": "url-only",
                "name": "URL Only",
                "base_url": "https://opencode.ai/zen/v1",
                "keys": [super::pool_key("key-default", "Default", "sk-url-secret", true)],
                "protocol": "chat_completions",
                "mappings": []
            }
        ],
        "model_prices": [
            {"provider_id": "zen-bound", "upstream_model": chat, "input": 1.0},
            {"provider_id": "zen-bound", "upstream_model": LEGACY_IMPLICIT_MODEL, "input": 2.0}
        ],
        "provider_templates": [
            {
                "template_id": "opencode-zen",
                "template": {
                    "id": "opencode-zen",
                    "name": "OpenCode Zen",
                    "base_url": "https://opencode.ai/zen/v1",
                    "protocol": "chat_completions",
                    "source": "https://opencode.ai/zen/v1/models",
                    "models_url": "https://opencode.ai/zen/v1/models",
                    "models": []
                },
                "synced_at": 123456789,
                "source": "https://opencode.ai/zen/v1/models"
            }
        ]
    })
}

/// AC-018 / AC-019: the first schema-3 read rewrites the Zen identity and bound
/// provider to Go, replaces the snapshot with the Go baseline, clears the sync
/// freshness, and preserves/quarantines provider data exactly as specified.
#[test]
fn zen_schema2_binding_migrates_to_go_and_preserves_user_data() {
    let _home = isolated_temp_home("migration-zen-to-go");
    let ids = go_baseline_ids();
    write_encrypted_config(&zen_schema2_fixture(&ids));

    let loaded = read_config().expect("the v2 configuration must migrate and read");

    // Template identity + snapshot replacement.
    assert!(
        !loaded
            .provider_templates
            .iter()
            .any(|state| state.template_id == "opencode-zen"),
        "the old Zen template identity must not survive"
    );
    let go_state = loaded
        .provider_templates
        .iter()
        .find(|state| state.template_id == "opencode-go")
        .expect("the migrated template state must be keyed opencode-go");
    assert_eq!(
        go_state.synced_at, None,
        "the Zen sync freshness must be cleared"
    );
    let go_template = go_state
        .template
        .as_ref()
        .expect("the Go baseline snapshot must replace the Zen snapshot");
    assert_eq!(go_template.id, "opencode-go");
    assert_eq!(
        go_template.models_url.as_deref(),
        Some("https://opencode.ai/zen/go/v1/models")
    );
    assert_eq!(go_template.base_url, "https://opencode.ai/zen/go/v1");
    assert!(go_template
        .models
        .iter()
        .any(|model| model.upstream_model == ids.chat));
    assert!(go_template
        .models
        .iter()
        .any(|model| model.upstream_model == ids.responses));

    // Provider binding + base URL.
    let provider = loaded
        .providers
        .iter()
        .find(|provider| provider.id == "zen-bound")
        .expect("the bound provider must be preserved");
    assert_eq!(provider.template_id.as_deref(), Some("opencode-go"));
    assert_eq!(provider.base_url, "https://opencode.ai/zen/go/v1");

    // Account/user-maintained fields preserved.
    assert_eq!(provider.name, "My Zen");
    assert_eq!(provider.weight, 7);
    assert_eq!(provider.tags, vec!["prod".to_string()]);
    assert_eq!(provider.icon.as_deref(), Some("custom-icon"));
    assert!(
        provider.enabled,
        "the provider's enabled intent must survive"
    );
    assert_eq!(provider.keys.len(), 1);
    assert_eq!(provider.keys[0].value, "sk-zen-secret");
    assert_eq!(
        provider.default_model, None,
        "an unsupported default model must be cleared"
    );

    // Supported mappings stay enabled with their declared protocol; legacy
    // mappings stay as disabled data and are recorded in ignored_models.
    let chat_mapping = provider
        .mappings
        .iter()
        .find(|mapping| mapping.upstream_model == ids.chat)
        .expect("the supported Chat mapping must be preserved");
    assert!(chat_mapping.enabled);
    let responses_mapping = provider
        .mappings
        .iter()
        .find(|mapping| mapping.upstream_model == ids.responses)
        .expect("the supported Responses mapping must be preserved");
    assert!(responses_mapping.enabled);
    assert_eq!(
        responses_mapping.protocol,
        Some(crate::ai_gateway::UpstreamProtocol::Responses),
        "a supported Responses mapping that inherited the old default must gain an explicit Responses protocol"
    );

    let legacy_implicit = provider
        .mappings
        .iter()
        .find(|mapping| mapping.upstream_model == LEGACY_IMPLICIT_MODEL)
        .expect("a legacy mapping outside the Go declarations must remain as disabled data");
    assert!(
        !legacy_implicit.enabled,
        "a legacy mapping outside the Go declarations must be disabled"
    );
    assert!(
        provider
            .ignored_models
            .iter()
            .any(|model| model == LEGACY_IMPLICIT_MODEL),
        "a quarantined legacy mapping must be recorded in ignored_models"
    );
    let legacy_explicit = provider
        .mappings
        .iter()
        .find(|mapping| mapping.upstream_model == LEGACY_EXPLICIT_MODEL)
        .expect("the explicit legacy mapping must be preserved as disabled data");
    assert!(!legacy_explicit.enabled);
    assert_eq!(
        legacy_explicit.protocol,
        Some(crate::ai_gateway::UpstreamProtocol::Responses),
        "an explicit mapping protocol must be preserved"
    );

    // Price rows preserved.
    assert!(loaded
        .model_prices
        .iter()
        .any(|row| row.upstream_model == ids.chat));
    assert!(loaded
        .model_prices
        .iter()
        .any(|row| row.upstream_model == LEGACY_IMPLICIT_MODEL));

    // URL-only unbound provider untouched.
    let url_only = loaded
        .providers
        .iter()
        .find(|provider| provider.id == "url-only")
        .expect("the URL-only provider must be preserved");
    assert_eq!(
        url_only.template_id, None,
        "a URL-only unbound provider must not be migrated"
    );
    assert_eq!(url_only.base_url, "https://opencode.ai/zen/v1");
}

/// AC-020: the second read is byte-stable, and the on-disk state carries the
/// migrated identity and the current schema version.
#[test]
fn zen_schema2_migration_is_idempotent_and_byte_stable() {
    let _home = isolated_temp_home("migration-zen-idempotent");
    let ids = go_baseline_ids();
    write_encrypted_config(&zen_schema2_fixture(&ids));

    let _first = read_config().expect("the first read must migrate");
    let bytes_after_first = raw_config_bytes();
    let on_disk = decrypted_config_json();
    assert_eq!(
        on_disk.get("schema_version").and_then(Value::as_u64),
        Some(u64::from(ZEN_TO_GO_SCHEMA_VERSION)),
        "the migration must stamp schema version 3: {on_disk}"
    );
    assert!(
        !on_disk["provider_templates"]
            .as_array()
            .expect("provider_templates array")
            .iter()
            .any(|state| state["template_id"] == json!("opencode-zen")),
        "the persisted Zen template must be replaced"
    );

    let second = read_config().expect("the second read must succeed");
    assert!(
        second
            .provider_templates
            .iter()
            .any(|state| state.template_id == "opencode-go"),
        "the second read must keep the Go identity"
    );
    assert_eq!(
        raw_config_bytes(),
        bytes_after_first,
        "the second read must not rewrite the migrated bytes"
    );
}

/// AC-020: a deleted old Zen template id maps deterministically to Go.
#[test]
fn deleted_zen_template_state_maps_deterministically_to_go() {
    let _home = isolated_temp_home("migration-zen-deleted");
    write_encrypted_config(&json!({
        "schema_version": 2,
        "deleted_template_ids": ["opencode-zen"],
        "providers": [{
            "id": "p",
            "name": "P",
            "base_url": "https://opencode.ai/zen/v1",
            "keys": [super::pool_key("key-default", "Default", "sk-p", true)],
            "protocol": "chat_completions",
            "template_id": "opencode-zen",
            "mappings": []
        }]
    }));

    let loaded = read_config().expect("the deleted-template config must migrate");
    assert!(
        !loaded
            .deleted_template_ids
            .iter()
            .any(|id| id == "opencode-zen"),
        "the deleted Zen identity must not survive"
    );
    assert!(
        loaded
            .deleted_template_ids
            .iter()
            .any(|id| id == "opencode-go"),
        "the deleted Zen identity must map to Go: {:?}",
        loaded.deleted_template_ids
    );
    let provider = loaded
        .providers
        .iter()
        .find(|provider| provider.id == "p")
        .expect("the provider must be preserved");
    assert_eq!(provider.template_id.as_deref(), Some("opencode-go"));
}

/// A schema-2 configuration that already carries both an `opencode-zen` and an
/// `opencode-go` template state, with one provider bound to `opencode-zen`.
fn zen_and_go_duplicate_state_fixture() -> Value {
    json!({
        "schema_version": 2,
        "enabled": true,
        "providers": [{
            "id": "zen-bound",
            "name": "Dual Bound",
            "base_url": "https://opencode.ai/zen/v1",
            "keys": [super::pool_key("key-default", "Default", "sk-dual", true)],
            "protocol": "chat_completions",
            "template_id": "opencode-zen",
            "mappings": []
        }],
        "provider_templates": [
            {
                "template_id": "opencode-zen",
                "synced_at": 111,
                "source": "https://opencode.ai/zen/v1/models",
                "template": {
                    "id": "opencode-zen",
                    "name": "OpenCode Zen",
                    "base_url": "https://opencode.ai/zen/v1",
                    "protocol": "chat_completions",
                    "source": "https://opencode.ai/zen/v1/models",
                    "models_url": "https://opencode.ai/zen/v1/models",
                    "models": []
                }
            },
            {
                "template_id": "opencode-go",
                "synced_at": 222,
                "source": "https://opencode.ai/zen/go/v1/models",
                "template": {
                    "id": "opencode-go",
                    "name": "OpenCode Go",
                    "base_url": "https://opencode.ai/zen/go/v1",
                    "protocol": "chat_completions",
                    "source": "https://opencode.ai/zen/go/v1/models",
                    "models_url": "https://opencode.ai/zen/go/v1/models",
                    "models": []
                }
            }
        ]
    })
}

/// AC-020: when a schema-2 config already carries an `opencode-go` state, the
/// migration drops the stale `opencode-zen` duplicate instead of producing two
/// Go identities, rebinds the Zen-bound provider to Go with the Go base URL, and
/// a second read is byte-stable.
#[test]
fn existing_go_state_drops_stale_zen_duplicate_and_rebinds_provider() {
    let _home = isolated_temp_home("migration-zen-duplicate-go");
    write_encrypted_config(&zen_and_go_duplicate_state_fixture());

    let loaded = read_config().expect("the schema-2 duplicate-state config must migrate and read");

    let go_state_count = loaded
        .provider_templates
        .iter()
        .filter(|state| state.template_id == "opencode-go")
        .count();
    assert_eq!(
        go_state_count, 1,
        "exactly one opencode-go state must remain after the migration"
    );
    assert!(
        !loaded
            .provider_templates
            .iter()
            .any(|state| state.template_id == "opencode-zen"),
        "the stale Zen duplicate must be dropped"
    );

    let provider = loaded
        .providers
        .iter()
        .find(|provider| provider.id == "zen-bound")
        .expect("the bound provider must be preserved");
    assert_eq!(
        provider.template_id.as_deref(),
        Some("opencode-go"),
        "the Zen-bound provider must be rebound to Go"
    );
    assert_eq!(
        provider.base_url, "https://opencode.ai/zen/go/v1",
        "the rebound provider must use the Go base URL"
    );

    // The rewrite persists the current version with exactly one Go state.
    let on_disk = decrypted_config_json();
    assert_eq!(
        on_disk.get("schema_version").and_then(Value::as_u64),
        Some(u64::from(ZEN_TO_GO_SCHEMA_VERSION)),
        "the migration must stamp schema version 3: {on_disk}"
    );
    let on_disk_go_states = on_disk["provider_templates"]
        .as_array()
        .expect("provider_templates array")
        .iter()
        .filter(|state| state["template_id"] == json!("opencode-go"))
        .count();
    assert_eq!(
        on_disk_go_states, 1,
        "the persisted configuration must carry exactly one Go state: {on_disk}"
    );

    // Idempotent: the second read writes nothing.
    let bytes_after_first = raw_config_bytes();
    let _second = read_config().expect("the second read must succeed");
    assert_eq!(
        raw_config_bytes(),
        bytes_after_first,
        "a second read must not rewrite the migrated bytes"
    );
}

/// AC-021: a failed migration rewrite returns the migrated in-memory view, keeps
/// the previous complete bytes on disk and completes on a later read.
#[cfg(unix)]
#[test]
fn zen_migration_failed_rewrite_keeps_old_bytes_and_retries() {
    use std::os::unix::fs::PermissionsExt;

    let _home = isolated_temp_home("migration-zen-readonly");
    let ids = go_baseline_ids();
    write_encrypted_config(&zen_schema2_fixture(&ids));
    let legacy_bytes = raw_config_bytes();

    let dir = crate::config::get_app_dir().expect("app dir");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o500))
        .expect("make the app directory read-only");
    let _restore = RestoreDirPermissions(dir.clone());

    let migrated = read_config().expect("the read must still return the migrated view");
    assert!(
        migrated
            .provider_templates
            .iter()
            .any(|state| state.template_id == "opencode-go"),
        "the failed rewrite must still return the migrated in-memory view"
    );
    assert_eq!(
        raw_config_bytes(),
        legacy_bytes,
        "a failed rewrite must keep the previous complete bytes on disk"
    );

    drop(_restore);
    let completed = read_config().expect("the retry read must succeed");
    assert!(
        completed
            .provider_templates
            .iter()
            .any(|state| state.template_id == "opencode-go"),
        "the retry must complete the migration"
    );
    assert_eq!(
        decrypted_config_json()
            .get("schema_version")
            .and_then(Value::as_u64),
        Some(u64::from(ZEN_TO_GO_SCHEMA_VERSION)),
        "the retry must persist schema version 3"
    );
}

/// AC-021: a schema version greater than 3 is read without downgrade, rewrite or
/// Zen-to-Go transformation.
#[test]
fn future_schema_version_is_neither_downgraded_nor_migrated() {
    let _home = isolated_temp_home("migration-future-v4");
    write_encrypted_config(&json!({
        "schema_version": crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION + 1,
        "provider_templates": [{
            "template_id": "opencode-zen",
            "template": {
                "id": "opencode-zen",
                "name": "OpenCode Zen",
                "base_url": "https://opencode.ai/zen/v1",
                "protocol": "chat_completions",
                "source": "https://opencode.ai/zen/v1/models",
                "models_url": "https://opencode.ai/zen/v1/models",
                "models": []
            },
            "synced_at": 5
        }],
        "providers": [{
            "id": "p",
            "name": "P",
            "base_url": "https://opencode.ai/zen/v1",
            "keys": [],
            "template_id": "opencode-zen",
            "mappings": []
        }],
        "future_only_field": {"kept": true}
    }));
    let before = raw_config_bytes();

    let loaded = read_config().expect("a future version must stay readable");
    assert_eq!(
        loaded.schema_version,
        crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION + 1,
        "the future version must be preserved"
    );
    assert!(
        loaded
            .provider_templates
            .iter()
            .any(|state| state.template_id == "opencode-zen"),
        "a future version must not run the Zen-to-Go migration"
    );
    assert_eq!(
        raw_config_bytes(),
        before,
        "a future version must never be rewritten or downgraded"
    );
}

/// AC-022: an older configuration whose template predates the policy fields
/// reads with the generic defaults and keeps unrelated data readable.
#[test]
fn older_config_missing_policy_fields_reads_with_generic_defaults() {
    let _home = isolated_temp_home("migration-missing-policy");
    write_encrypted_config(&json!({
        "schema_version": 2,
        "providers": [{
            "id": "p",
            "name": "P",
            "base_url": "https://p.example.com/v1",
            "keys": [super::pool_key("key-default", "Default", "sk-p", true)],
            "protocol": "chat_completions",
            "mappings": [{"local_model": "m", "upstream_model": "m", "enabled": true}]
        }],
        "provider_templates": [{
            "template_id": "custom-old",
            "template": {
                "id": "custom-old",
                "name": "Old",
                "base_url": "https://old.example.com/v1",
                "protocol": "chat_completions",
                "source": "",
                "models_url": null,
                "models": []
            },
            "synced_at": 9
        }],
        "unrelated_unknown_field": {"kept": true}
    }));

    let loaded = read_config().expect("an older config must stay readable");
    let state = loaded
        .provider_templates
        .iter()
        .find(|state| state.template_id == "custom-old")
        .expect("the custom template must survive");
    let template = state
        .template
        .as_ref()
        .expect("the snapshot must be present");
    let value = serde_json::to_value(template).expect("the template must serialize");
    assert_eq!(value["sync_strategy"], json!("generic"));
    assert_eq!(value["quota_strategy"], json!("none"));
    assert_eq!(value["error_strategy"], json!("generic"));
    assert!(
        loaded.providers.iter().any(|provider| provider.id == "p"),
        "unrelated supported data must remain readable"
    );
}

/// A schema-3 configuration whose `provider_templates` mixes one template with
/// unknown strategy values and one with valid values, plus unrelated provider
/// and price data that must all survive the tolerant read.
fn unknown_strategy_config_fixture() -> Value {
    json!({
        "schema_version": 3,
        "enabled": true,
        "providers": [{
            "id": "p-keep",
            "name": "Keep Provider",
            "base_url": "https://keep.example.com/v1",
            "keys": [super::pool_key("key-default", "Default", "sk-keep", true)],
            "protocol": "chat_completions",
            "mappings": [{"local_model": "m", "upstream_model": "m", "enabled": true}]
        }],
        "model_prices": [{"provider_id": "p-keep", "upstream_model": "m", "input": 1.0}],
        "provider_templates": [
            {
                "template_id": "future-policy",
                "synced_at": null,
                "template": {
                    "id": "future-policy",
                    "name": "Future Policy",
                    "base_url": "https://future.example.com/v1",
                    "protocol": "chat_completions",
                    "sync_strategy": "future_mode",
                    "quota_strategy": "future_monitor",
                    "error_strategy": "future_policy",
                    "source": "",
                    "models_url": null,
                    "models": []
                }
            },
            {
                "template_id": "valid-policy",
                "synced_at": null,
                "template": {
                    "id": "valid-policy",
                    "name": "Valid Policy",
                    "base_url": "https://valid.example.com/v1",
                    "protocol": "chat_completions",
                    "sync_strategy": "opencode_go",
                    "quota_strategy": "commandcode",
                    "error_strategy": "generic",
                    "source": "",
                    "models_url": null,
                    "models": []
                }
            }
        ]
    })
}

/// REQ-001 / AC-001: a persisted configuration whose template carries unknown
/// strategy values must still read as a whole; the unknown values resolve to
/// `generic`/`none`/`generic` while a sibling template's valid values survive
/// and unrelated configuration data is untouched.
#[test]
fn persisted_config_with_unknown_strategy_values_reads_with_generic_defaults() {
    let _home = isolated_temp_home("migration-unknown-strategy");
    write_encrypted_config(&unknown_strategy_config_fixture());

    let loaded = read_config().expect(
        "a persisted config whose template carries unknown strategy values must still read",
    );

    let template_for = |id: &str| {
        loaded
            .provider_templates
            .iter()
            .find(|state| state.template_id == id)
            .and_then(|state| state.template.clone())
            .unwrap_or_else(|| panic!("the {id} template state must survive"))
    };

    let unknown = template_for("future-policy");
    assert_eq!(
        unknown.sync_strategy,
        crate::ai_gateway::SyncStrategy::Generic,
        "an unknown sync strategy must resolve to generic"
    );
    assert_eq!(
        unknown.quota_strategy,
        crate::ai_gateway::QuotaStrategy::NoQuota,
        "an unknown quota strategy must resolve to none"
    );
    assert_eq!(
        unknown.error_strategy,
        crate::ai_gateway::ErrorStrategy::Generic,
        "an unknown error strategy must resolve to generic"
    );

    let valid = template_for("valid-policy");
    assert_eq!(
        valid.sync_strategy,
        crate::ai_gateway::SyncStrategy::OpencodeGo,
        "a valid sync strategy must survive the tolerant read"
    );
    assert_eq!(
        valid.quota_strategy,
        crate::ai_gateway::QuotaStrategy::Commandcode,
        "a valid quota strategy must survive the tolerant read"
    );
    assert_eq!(
        valid.error_strategy,
        crate::ai_gateway::ErrorStrategy::Generic,
        "a valid error strategy must survive the tolerant read"
    );

    let provider = loaded
        .providers
        .iter()
        .find(|provider| provider.id == "p-keep")
        .expect("unrelated provider data must survive the tolerant read");
    assert_eq!(provider.name, "Keep Provider");
    assert_eq!(provider.base_url, "https://keep.example.com/v1");
    assert!(
        loaded.model_prices.iter().any(|row| {
            row.provider_id.as_deref() == Some("p-keep") && row.upstream_model == "m"
        }),
        "unrelated price rows must survive the tolerant read"
    );
    assert_eq!(
        loaded.schema_version,
        crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION,
        "the stored current schema version must be preserved"
    );
}

/// The current configuration schema is pinned to 9 for the Zen Free inference
/// endpoint change.
#[test]
fn gateway_config_schema_version_advances_to_nine() {
    assert_eq!(
        crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION,
        9,
        "the Zen Free inference endpoint change must be gated by schema version 9"
    );
}

// ---------------------------------------------------------------------------
// Usage-database version 2: one-shot backfill, coverage and downgrade safety
// (plan Step 1, REQ-004 / AC-004 / AC-005)
// ---------------------------------------------------------------------------

/// Create a raw usage database at an explicit `PRAGMA user_version` carrying
/// every current column plus the additive accounting columns; the permanent
/// migration itself adds any table or column it still needs.
fn create_versioned_usage_db(path: &Path, user_version: i64) {
    let connection = Connection::open(path).expect("open raw usage db");
    connection
        .execute_batch(
            "CREATE TABLE usage_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp_ms INTEGER NOT NULL,
                local_model TEXT NOT NULL,
                upstream_model TEXT NOT NULL,
                provider_id TEXT NOT NULL,
                provider_name TEXT NOT NULL,
                result TEXT NOT NULL,
                status INTEGER NOT NULL,
                input_tokens INTEGER NOT NULL,
                cache_read_tokens INTEGER NOT NULL,
                cache_write_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                amount REAL,
                duration_ms INTEGER NOT NULL,
                error_message TEXT,
                terminal INTEGER NOT NULL DEFAULT 1,
                reasoning_effort TEXT,
                usage_semantics TEXT NOT NULL DEFAULT 'legacy',
                usage_present INTEGER NOT NULL DEFAULT 0,
                cache_accounting_valid INTEGER NOT NULL DEFAULT 0,
                cost_breakdown TEXT
            );
            CREATE INDEX idx_usage_logs_timestamp ON usage_logs(timestamp_ms);
            CREATE INDEX idx_usage_logs_local_model ON usage_logs(local_model);",
        )
        .expect("create raw usage schema");
    connection
        .pragma_update(None, "user_version", user_version)
        .expect("set usage db version");
}

/// A raw detail row used to construct pre-upgrade fixtures.
struct RawUsageRow {
    timestamp_ms: i64,
    local_model: String,
    upstream_model: String,
    provider_id: String,
    provider_name: String,
    result: String,
    terminal: bool,
    amount: Option<f64>,
    input_tokens: i64,
    output_tokens: i64,
    semantics: String,
    present: i64,
    valid: i64,
}

fn usage_row(
    timestamp_ms: i64,
    provider_id: &str,
    result: &str,
    terminal: bool,
    amount: Option<f64>,
    input_tokens: i64,
    output_tokens: i64,
) -> RawUsageRow {
    RawUsageRow {
        timestamp_ms,
        local_model: "local-raw".to_string(),
        upstream_model: "remote-raw".to_string(),
        provider_id: provider_id.to_string(),
        provider_name: if provider_id.is_empty() {
            String::new()
        } else {
            format!("Provider {provider_id}")
        },
        result: result.to_string(),
        terminal,
        amount,
        input_tokens,
        output_tokens,
        semantics: "canonical_v1".to_string(),
        present: 1,
        valid: 1,
    }
}

fn insert_usage_row(path: &Path, row: &RawUsageRow) {
    let connection = Connection::open(path).expect("open raw usage db");
    let total_tokens = row.input_tokens + row.output_tokens;
    connection
        .execute(
            "INSERT INTO usage_logs (
                timestamp_ms, local_model, upstream_model, provider_id, provider_name,
                result, status, input_tokens, cache_read_tokens, cache_write_tokens,
                output_tokens, total_tokens, amount, duration_ms, error_message, terminal,
                reasoning_effort, usage_semantics, usage_present, cache_accounting_valid, cost_breakdown
            ) VALUES (?, ?, ?, ?, ?, ?, 200, ?, 0, 0, ?, ?, ?, 1, NULL, ?, NULL, ?, ?, ?, NULL)",
            rusqlite::params![
                row.timestamp_ms,
                row.local_model,
                row.upstream_model,
                row.provider_id,
                row.provider_name,
                row.result,
                row.input_tokens,
                row.output_tokens,
                total_tokens,
                row.amount,
                if row.terminal { 1i64 } else { 0i64 },
                row.semantics,
                row.present,
                row.valid,
            ],
        )
        .expect("insert raw usage row");
}

fn raw_user_version(path: &Path) -> i64 {
    let connection = Connection::open(path).expect("open raw usage db");
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read user_version")
}

fn has_table(path: &Path, name: &str) -> bool {
    let connection = Connection::open(path).expect("open raw usage db");
    connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
            [name],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .unwrap_or(false)
}

/// Every distinct persisted summary provider ID, ascending.
fn summary_provider_ids(path: &Path) -> Vec<String> {
    let connection = Connection::open(path).expect("open raw usage db");
    let mut statement = connection
        .prepare("SELECT DISTINCT provider_id FROM provider_daily_usage ORDER BY provider_id ASC")
        .expect("prepare summary ids");
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query summary ids");
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect summary ids")
}

/// Count of persisted summary rows for one provider ID.
fn summary_row_count(path: &Path, provider_id: &str) -> i64 {
    let connection = Connection::open(path).expect("open raw usage db");
    connection
        .query_row(
            "SELECT COUNT(*) FROM provider_daily_usage WHERE provider_id = ?",
            [provider_id],
            |row| row.get(0),
        )
        .expect("count summary rows")
}

/// Count of retained detail rows, read without re-running the migration.
fn raw_usage_log_count(path: &Path) -> i64 {
    let connection = Connection::open(path).expect("open raw usage db");
    connection
        .query_row("SELECT COUNT(*) FROM usage_logs", [], |row| row.get(0))
        .expect("count usage rows")
}

/// AC-005/AC-004: a version-0 database containing legacy and canonical terminal
/// rows, superseded attempts, an empty-provider failure and a cancelled row is
/// backfilled exactly once. Only retained terminal, valid-result, nonblank
/// provider rows contribute; original fields are never rewritten; the version-0
/// cancelled cleanup still runs; and the earliest available day and the
/// continuity boundary become visible.
#[test]
fn usage_database_v0_backfills_once_and_preserves_original_fields() {
    set_usage_clock_for_test(super::rfc3339_millis("2026-09-20T10:00:00+08:00"));
    let dir = super::make_temp_dir("migration-v0-backfill");
    fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("ai_gateway_usage.db");
    create_versioned_usage_db(&path, 0);

    let legacy_at = super::rfc3339_millis("2026-09-09T12:00:00+08:00");
    let canonical_at = super::rfc3339_millis("2026-09-10T12:00:00+08:00");
    let attempt_at = super::rfc3339_millis("2026-09-10T13:00:00+08:00");
    let cancelled_at = super::rfc3339_millis("2026-09-10T14:00:00+08:00");
    let failure_at = super::rfc3339_millis("2026-09-11T12:00:00+08:00");

    let mut legacy = usage_row(legacy_at, "p1", "success", true, None, 4, 1);
    legacy.semantics = "legacy".to_string();
    legacy.present = 0;
    legacy.valid = 0;
    insert_usage_row(&path, &legacy);
    insert_usage_row(
        &path,
        &usage_row(canonical_at, "p1", "success", true, Some(0.5), 10, 5),
    );
    insert_usage_row(&path, &usage_row(failure_at, "p1", "failure", true, None, 5, 0));
    insert_usage_row(
        &path,
        &usage_row(attempt_at, "p1", "success", false, Some(9.9), 50, 50),
    );
    insert_usage_row(
        &path,
        &usage_row(
            super::rfc3339_millis("2026-09-12T12:00:00+08:00"),
            "",
            "failure",
            true,
            None,
            0,
            0,
        ),
    );
    // A whitespace-only provider id is not a real identity either: the backfill
    // must skip it exactly like an empty id, never key a summary on it.
    insert_usage_row(
        &path,
        &usage_row(
            super::rfc3339_millis("2026-09-13T12:00:00+08:00"),
            "   ",
            "success",
            true,
            Some(0.5),
            10,
            5,
        ),
    );
    insert_usage_row(
        &path,
        &usage_row(cancelled_at, "p1", "cancelled", true, None, 0, 0),
    );
    assert_eq!(count_cancelled(&path), 1, "the fixture carries a cancelled row");

    assert_eq!(
        USAGE_DB_VERSION, 2,
        "Step 1 advances the usage database to version 2"
    );

    let store = UsageLogStore::at(&path);
    let now = super::rfc3339_millis("2026-09-20T12:00:00+08:00");
    let history = store
        .provider_usage_history("p1", now)
        .expect("backfilled provider history");

    assert_eq!(raw_user_version(&path), 2, "the marker advances with the backfill");
    assert_eq!(count_cancelled(&path), 0, "version 0 still runs the cancelled cleanup");
    assert!(
        has_table(&path, "provider_usage_metadata"),
        "the migration records coverage metadata"
    );
    assert_eq!(
        summary_provider_ids(&path),
        vec!["p1".to_string()],
        "a whitespace-only historical provider id must never be backfilled"
    );

    assert_eq!(history.earliest_available_day.as_deref(), Some("2026-09-09"));
    assert_eq!(
        history.continuity_start_day.as_deref(),
        Some("2026-09-20"),
        "the migration day is the continuous-recording boundary"
    );
    assert_eq!(history.end_day.as_deref(), Some("2026-09-20"));

    let cumulative = history.cumulative.expect("cumulative");
    assert_eq!(
        cumulative.request_count, 3,
        "legacy + canonical successes and the failure contribute; the attempt and the empty provider do not"
    );
    assert_eq!(cumulative.input_tokens, 19);
    assert_eq!(cumulative.output_tokens, 6);
    assert_eq!(cumulative.total_tokens, 25);
    assert_eq!(cumulative.unpriced_count, 2);
    assert_eq!(
        cumulative.cache_eligible_tokens, 10,
        "only the canonical valid row is cache eligible"
    );
    assert_eq!(cumulative.cache_hit_tokens, 0);
    assert_eq!(cumulative.cache_hit_rate_percent, Some(0.0));

    // Original detail fields are preserved untouched.
    let detail = store.all_records().expect("detail rows");
    assert_eq!(detail.len(), 6, "six rows survive the cancelled-row cleanup");
    assert!(
        detail.iter().any(|row| row.provider_id == "   "),
        "the whitespace-only detail row must be preserved untouched"
    );
    let canonical = detail
        .iter()
        .find(|row| row.timestamp_ms == canonical_at)
        .expect("canonical row");
    assert_eq!(canonical.amount, Some(0.5));
    assert_eq!(canonical.total_tokens, 15);
    assert_eq!(canonical.provider_name, "Provider p1");
    assert!(canonical.terminal);
    assert!(detail.iter().any(|row| !row.terminal), "the attempt stays detail-only");

    // A second open re-reads without backfilling twice.
    let reopened = UsageLogStore::at(&path)
        .provider_usage_history("p1", now)
        .expect("reopened history");
    assert_eq!(
        reopened.cumulative.expect("cumulative").request_count,
        3,
        "the backfill is one-shot"
    );

    clear_usage_clock_for_test();
    let _ = fs::remove_dir_all(&dir);
}

/// AC-005: a version-1 database already ran the cancelled cleanup, so its
/// cancelled rows survive while the version-2 backfill still runs once.
#[test]
fn usage_database_v1_skips_cancelled_cleanup_and_backfills() {
    set_usage_clock_for_test(super::rfc3339_millis("2026-09-20T10:00:00+08:00"));
    let dir = super::make_temp_dir("migration-v1-backfill");
    fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("ai_gateway_usage.db");
    create_versioned_usage_db(&path, 1);
    insert_usage_row(
        &path,
        &usage_row(
            super::rfc3339_millis("2026-09-10T12:00:00+08:00"),
            "p1",
            "success",
            true,
            Some(0.5),
            10,
            5,
        ),
    );
    insert_usage_row(
        &path,
        &usage_row(
            super::rfc3339_millis("2026-09-10T14:00:00+08:00"),
            "p1",
            "cancelled",
            true,
            None,
            0,
            0,
        ),
    );

    let store = UsageLogStore::at(&path);
    let now = super::rfc3339_millis("2026-09-20T12:00:00+08:00");
    let history = store
        .provider_usage_history("p1", now)
        .expect("backfilled history");

    assert_eq!(raw_user_version(&path), 2);
    assert_eq!(
        count_cancelled(&path),
        1,
        "the version-0 cancelled cleanup must not rerun for version 1"
    );
    assert_eq!(history.earliest_available_day.as_deref(), Some("2026-09-10"));
    assert_eq!(
        history.cumulative.expect("cumulative").request_count,
        1,
        "only the retained terminal success counts"
    );

    clear_usage_clock_for_test();
    let _ = fs::remove_dir_all(&dir);
}

/// AC-005: a controlled migration failure (read-only directory) leaves the
/// version marker unadvanced and no summary table behind; restoring write
/// access lets a later open complete the migration.
#[cfg(unix)]
#[test]
fn failed_usage_migration_leaves_no_partial_summary_and_retries() {
    use std::os::unix::fs::PermissionsExt;

    set_usage_clock_for_test(super::rfc3339_millis("2026-09-20T10:00:00+08:00"));
    let dir = super::make_temp_dir("migration-failed-retry");
    fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("ai_gateway_usage.db");
    create_versioned_usage_db(&path, 0);
    insert_usage_row(
        &path,
        &usage_row(
            super::rfc3339_millis("2026-09-10T12:00:00+08:00"),
            "p1",
            "success",
            true,
            Some(0.5),
            10,
            5,
        ),
    );

    let store = UsageLogStore::at(&path);
    let now = super::rfc3339_millis("2026-09-20T12:00:00+08:00");

    fs::set_permissions(&dir, fs::Permissions::from_mode(0o500))
        .expect("make the temp directory read-only");
    let restore = RestoreDirPermissions(dir.clone());
    let failed = store
        .provider_usage_history("p1", now)
        .expect_err("a read-only database cannot complete the migration");
    assert!(!failed.is_empty(), "the failure must carry a message");
    assert_eq!(
        raw_user_version(&path),
        0,
        "a failed migration must not advance the marker"
    );
    assert!(
        !has_table(&path, "provider_daily_usage"),
        "a failed migration must leave no summary table"
    );

    drop(restore);
    let history = store
        .provider_usage_history("p1", now)
        .expect("the retried migration must succeed");
    assert_eq!(raw_user_version(&path), 2);
    assert_eq!(history.earliest_available_day.as_deref(), Some("2026-09-10"));
    assert_eq!(history.cumulative.expect("cumulative").request_count, 1);

    clear_usage_clock_for_test();
    let _ = fs::remove_dir_all(&dir);
}

/// AC-005: a future database version is never downgraded and is not backfilled.
#[test]
fn future_usage_database_version_is_not_downgraded() {
    set_usage_clock_for_test(super::rfc3339_millis("2026-09-20T10:00:00+08:00"));
    let dir = super::make_temp_dir("migration-future-version");
    fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("ai_gateway_usage.db");
    create_versioned_usage_db(&path, 99);
    let row_at = super::rfc3339_millis("2026-09-10T12:00:00+08:00");
    insert_usage_row(&path, &usage_row(row_at, "p1", "success", true, Some(0.5), 10, 5));

    let store = UsageLogStore::at(&path);
    let now = super::rfc3339_millis("2026-09-20T12:00:00+08:00");
    let history = store
        .provider_usage_history("p1", now)
        .expect("a future-version database stays readable");

    assert_eq!(
        raw_user_version(&path),
        99,
        "a future version must be preserved, never downgraded"
    );
    assert!(
        history.days.is_empty(),
        "a future-version database must not be backfilled: {history:?}"
    );
    assert!(
        store
            .all_records()
            .expect("future-version detail rows")
            .iter()
            .any(|row| row.timestamp_ms == row_at),
        "the future-version detail rows must be preserved"
    );

    clear_usage_clock_for_test();
    let _ = fs::remove_dir_all(&dir);
}

/// AC-004/AC-005: concurrent first opens of the same version-0 database
/// backfill once; the marker and the summary are never duplicated.
#[test]
fn concurrent_usage_backfill_is_idempotent() {
    set_usage_clock_for_test(super::rfc3339_millis("2026-09-20T10:00:00+08:00"));
    let dir = super::make_temp_dir("migration-concurrent-backfill");
    fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("ai_gateway_usage.db");
    create_versioned_usage_db(&path, 0);
    insert_usage_row(
        &path,
        &usage_row(
            super::rfc3339_millis("2026-09-10T12:00:00+08:00"),
            "p1",
            "success",
            true,
            Some(0.5),
            10,
            5,
        ),
    );

    let now = super::rfc3339_millis("2026-09-20T12:00:00+08:00");
    let barrier = Barrier::new(3);
    let counts: Vec<Result<u32, String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..3)
            .map(|_| {
                scope.spawn(|| -> Result<u32, String> {
                    let store = UsageLogStore::at(&path);
                    barrier.wait();
                    store.provider_usage_history("p1", now).map(|history| {
                        history
                            .cumulative
                            .map(|metrics| metrics.request_count)
                            .unwrap_or(0)
                    })
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("a concurrent open must not panic"))
            .collect()
    });

    for count in &counts {
        assert_eq!(
            *count.as_ref().expect("a concurrent open must succeed"),
            1,
            "every concurrent opener sees the single backfilled contribution"
        );
    }
    assert_eq!(raw_user_version(&path), 2);
    let final_history = UsageLogStore::at(&path)
        .provider_usage_history("p1", now)
        .expect("final history");
    assert_eq!(
        final_history.cumulative.expect("cumulative").request_count,
        1,
        "concurrent opens must not duplicate the backfill"
    );

    clear_usage_clock_for_test();
    let _ = fs::remove_dir_all(&dir);
}

/// AC-005: a fault *during* the version-2 backfill — after one grouped
/// contribution has already been inserted inside the migration transaction —
/// rolls the whole transaction back: the version marker stays unadvanced and no
/// partial summary row survives. Dropping the fault lets a later open backfill
/// both providers exactly once.
#[test]
fn failed_backfill_after_partial_contributions_rolls_back_and_retries() {
    set_usage_clock_for_test(super::rfc3339_millis("2026-09-20T10:00:00+08:00"));
    let dir = super::make_temp_dir("migration-backfill-partial-abort");
    fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("ai_gateway_usage.db");
    create_versioned_usage_db(&path, 0);
    insert_usage_row(
        &path,
        &usage_row(
            super::rfc3339_millis("2026-09-10T12:00:00+08:00"),
            "p1",
            "success",
            true,
            Some(0.5),
            10,
            5,
        ),
    );
    insert_usage_row(
        &path,
        &usage_row(
            super::rfc3339_millis("2026-09-11T12:00:00+08:00"),
            "p2",
            "success",
            true,
            Some(0.5),
            10,
            5,
        ),
    );

    // An already-initialized summary table (the state an interrupted attempt
    // leaves) plus a trigger that aborts the second grouped contribution, so
    // the fault lands after the first insert has already been applied.
    let trigger = Connection::open(&path).expect("open raw usage db");
    trigger
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS provider_daily_usage (
                provider_id TEXT NOT NULL,
                day TEXT NOT NULL,
                provider_name TEXT NOT NULL,
                request_count INTEGER NOT NULL DEFAULT 0,
                successful_request_count INTEGER NOT NULL DEFAULT 0,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                cache_read_tokens INTEGER NOT NULL DEFAULT 0,
                cache_write_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                amount REAL NOT NULL DEFAULT 0,
                unpriced_count INTEGER NOT NULL DEFAULT 0,
                cache_hit_tokens INTEGER NOT NULL DEFAULT 0,
                cache_eligible_tokens INTEGER NOT NULL DEFAULT 0,
                cache_rate_eligible_count INTEGER NOT NULL DEFAULT 0,
                first_request_at_ms INTEGER NOT NULL,
                last_request_at_ms INTEGER NOT NULL,
                PRIMARY KEY (provider_id, day)
            );
            CREATE TRIGGER abort_second_backfill_summary
            AFTER INSERT ON provider_daily_usage
            WHEN (SELECT COUNT(*) FROM provider_daily_usage) >= 2
            BEGIN SELECT RAISE(ABORT, 'injected backfill abort'); END;",
        )
        .expect("arm the partial-backfill abort");
    drop(trigger);

    let store = UsageLogStore::at(&path);
    let now = super::rfc3339_millis("2026-09-20T12:00:00+08:00");
    let failed = store
        .provider_usage_history("p1", now)
        .expect_err("the injected backfill abort must fail the open");
    assert!(!failed.is_empty(), "the failure must carry a message");
    assert_eq!(
        raw_user_version(&path),
        0,
        "a failed backfill must not advance the version marker"
    );
    assert_eq!(
        summary_row_count(&path, "p1"),
        0,
        "the contribution applied before the abort must be rolled back"
    );
    assert_eq!(
        summary_row_count(&path, "p2"),
        0,
        "the aborted contribution must not be stored"
    );
    assert_eq!(
        raw_usage_log_count(&path),
        2,
        "the failed migration must leave the retained detail rows unchanged"
    );

    // Dropping the fault lets a later open backfill both providers once.
    let clear = Connection::open(&path).expect("open raw usage db to drop the trigger");
    clear
        .execute_batch("DROP TRIGGER abort_second_backfill_summary")
        .expect("drop the partial-backfill abort");
    drop(clear);

    let retried = store
        .provider_usage_history("p1", now)
        .expect("the retried migration must succeed");
    assert_eq!(raw_user_version(&path), 2, "the retry advances the marker");
    assert_eq!(summary_row_count(&path, "p1"), 1);
    assert_eq!(summary_row_count(&path, "p2"), 1);
    assert_eq!(
        retried.cumulative.expect("p1 cumulative").request_count,
        1,
        "each provider is backfilled exactly once"
    );
    assert_eq!(
        store
            .provider_usage_history("p2", now)
            .expect("p2 history")
            .cumulative
            .expect("p2 cumulative")
            .request_count,
        1
    );

    clear_usage_clock_for_test();
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Ordered fallback models: the one-time migration of the legacy single
// provider-specific fallback into a gateway-model list (REQ-001 / REQ-005;
// AC-001 and the migration half of AC-007).
//
// Every fixture below is raw JSON, so the assertions observe the persisted
// shape instead of a typed member that does not exist yet. A RED is therefore
// an observable mismatch (the legacy key survives, the list is missing, the
// version is not stamped) and never a compilation failure.
// ---------------------------------------------------------------------------

/// The legacy single provider-specific fallback of the fixture below: a full
/// candidate reference carrying its own reasoning declaration and weight, both
/// of which a model-level list has no place for.
fn legacy_pool_fallback_candidate() -> Value {
    json!({
        "provider_id": "p-b",
        "local_model": "local-fallback",
        "upstream_model": "remote-fallback",
        "protocol": "chat_completions",
        "weight": 3,
        "reasoning": {
            "profile": "openai_effort",
            "values": ["high"],
            "supports_disable": false,
            "mappings": [{"effort": "high", "action": {"kind": "enum", "value": "high"}}]
        }
    })
}

/// A schema-3 configuration whose main pool carries the legacy single
/// provider-specific fallback, a regular candidate with its own reasoning
/// declaration and weight, a pool default reasoning preference, a sibling pool
/// without any fallback, two providers with their own credentials and one
/// price row.
fn legacy_pool_fallback_config() -> Value {
    json!({
        "schema_version": 3,
        "enabled": true,
        "port": 17688,
        "keys": [{
            "id": "k-local",
            "label": "k-local",
            "value": "local-key",
            "enabled": true,
            "created_at": 1
        }],
        "providers": [
            {
                "id": "p-a",
                "name": "Provider A",
                "base_url": "https://a.example.invalid/v1",
                "protocol": "chat_completions",
                "enabled": true,
                "keys": [{
                    "id": "key-a",
                    "name": "Key A",
                    "value": "sk-secret-a",
                    "enabled": true,
                    "auto_marked": false,
                    "failure_kind": null,
                    "marked_at": null,
                    "reason": null
                }],
                "mappings": [
                    {
                        "local_model": "local-regular",
                        "upstream_model": "remote-regular",
                        "protocol": "chat_completions",
                        "enabled": true
                    },
                    {
                        "local_model": "local-sibling",
                        "upstream_model": "remote-sibling",
                        "protocol": "chat_completions",
                        "enabled": true
                    }
                ]
            },
            {
                "id": "p-b",
                "name": "Provider B",
                "base_url": "https://b.example.invalid/v1",
                "protocol": "chat_completions",
                "enabled": true,
                "keys": [{
                    "id": "key-b",
                    "name": "Key B",
                    "value": "sk-secret-b",
                    "enabled": true,
                    "auto_marked": false,
                    "failure_kind": null,
                    "marked_at": null,
                    "reason": null
                }],
                "mappings": [{
                    "local_model": "local-fallback",
                    "upstream_model": "remote-fallback",
                    "protocol": "chat_completions",
                    "enabled": true
                }]
            }
        ],
        "model_prices": [{
            "provider_id": "p-a",
            "upstream_model": "remote-regular",
            "input": 1.5
        }],
        "model_pools": [
            {
                "id": "pool-main",
                "name": "Main Pool",
                "model": "main-model",
                "enabled": true,
                "default_reasoning_effort": "high",
                "candidates": [{
                    "provider_id": "p-a",
                    "local_model": "local-regular",
                    "upstream_model": "remote-regular",
                    "protocol": "chat_completions",
                    "weight": 7,
                    "reasoning": {
                        "profile": "openai_effort",
                        "values": ["high"],
                        "supports_disable": false,
                        "mappings": [{"effort": "high", "action": {"kind": "enum", "value": "high"}}]
                    }
                }],
                "fallback": legacy_pool_fallback_candidate()
            },
            {
                "id": "pool-sibling",
                "name": "Sibling Pool",
                "model": "sibling-model",
                "enabled": true,
                "candidates": [{
                    "provider_id": "p-a",
                    "local_model": "local-sibling",
                    "upstream_model": "remote-sibling",
                    "protocol": "chat_completions",
                    "weight": 2
                }]
            }
        ],
        "model_pool_reserved_models": ["retired-model"]
    })
}

/// The persisted pool row with the given id.
fn persisted_pool<'a>(on_disk: &'a Value, pool_id: &str) -> &'a Value {
    on_disk
        .get("model_pools")
        .and_then(Value::as_array)
        .and_then(|pools| {
            pools
                .iter()
                .find(|pool| pool.get("id").and_then(Value::as_str) == Some(pool_id))
        })
        .unwrap_or_else(|| panic!("missing the persisted pool '{pool_id}': {on_disk}"))
}

/// The persisted fallback model list of one pool. A pool without any stored
/// fallback model reads as an empty list.
fn persisted_fallback_models(on_disk: &Value, pool_id: &str) -> Vec<String> {
    persisted_pool(on_disk, pool_id)
        .get("fallback_models")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// REQ-001 / REQ-005 / AC-001: a schema-3 pool with one provider-specific
/// fallback migrates exactly once into a one-entry gateway-model list, stamps
/// schema 4 and keeps every unrelated pool value: the regular candidates, their
/// reasoning declarations and weights, the pool default reasoning preference,
/// the sibling pool, the retired alias list, the providers with their
/// credentials and the price table.
#[test]
fn legacy_pool_fallback_object_migrates_to_an_ordered_fallback_model_list() {
    let _home = isolated_temp_home("migration-legacy-pool-fallback");
    write_encrypted_config(&legacy_pool_fallback_config());

    let migrated = read_config().expect("the legacy configuration must stay readable");
    assert_eq!(
        migrated.schema_version, ORDERED_FALLBACK_SCHEMA_VERSION,
        "the migrated configuration must carry schema version 4"
    );

    let on_disk = decrypted_config_json();
    assert_eq!(
        on_disk.get("schema_version").and_then(Value::as_u64),
        Some(u64::from(ORDERED_FALLBACK_SCHEMA_VERSION)),
        "the migrated file must be stamped with schema version 4: {on_disk}"
    );

    let pool = persisted_pool(&on_disk, "pool-main");
    assert_eq!(
        pool.get("fallback_models"),
        Some(&json!(["local-fallback"])),
        "the legacy fallback must migrate to a one-entry gateway-model list: {pool}"
    );
    assert!(
        pool.get("fallback").is_none(),
        "the legacy fallback key must be removed by the migration: {pool}"
    );

    // The regular candidate keeps its identity, weight and reasoning
    // declaration, and the pool default preference stays untouched.
    let candidates = pool
        .get("candidates")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("the migrated pool must keep its candidates: {pool}"));
    assert_eq!(
        candidates.len(),
        1,
        "the regular candidates must survive the migration: {pool}"
    );
    let regular = &candidates[0];
    assert_eq!(regular.get("provider_id"), Some(&json!("p-a")));
    assert_eq!(regular.get("local_model"), Some(&json!("local-regular")));
    assert_eq!(regular.get("upstream_model"), Some(&json!("remote-regular")));
    assert_eq!(regular.get("protocol"), Some(&json!("chat_completions")));
    assert_eq!(
        regular.get("weight"),
        Some(&json!(7)),
        "the regular candidate's weight must be unchanged: {regular}"
    );
    assert_eq!(
        regular.get("reasoning").and_then(|reasoning| reasoning.get("profile")),
        Some(&json!("openai_effort")),
        "the regular candidate's reasoning declaration must be unchanged: {regular}"
    );
    assert_eq!(
        regular
            .get("reasoning")
            .and_then(|reasoning| reasoning.get("mappings"))
            .and_then(Value::as_array)
            .map(|mappings| mappings.len()),
        Some(1),
        "the regular candidate's reasoning mappings must be unchanged: {regular}"
    );
    assert_eq!(
        pool.get("default_reasoning_effort"),
        Some(&json!("high")),
        "the pool default reasoning preference must be unchanged: {pool}"
    );

    // A pool without a legacy fallback migrates to an empty list.
    assert!(
        persisted_fallback_models(&on_disk, "pool-sibling").is_empty(),
        "a pool without a legacy fallback must migrate to an empty list: {on_disk}"
    );

    // The unrelated pool, alias, provider and price values stay unchanged.
    let sibling = persisted_pool(&on_disk, "pool-sibling");
    assert_eq!(sibling.get("model"), Some(&json!("sibling-model")));
    assert_eq!(sibling.get("name"), Some(&json!("Sibling Pool")));
    assert_eq!(
        sibling
            .get("candidates")
            .and_then(Value::as_array)
            .map(|candidates| candidates[0].get("local_model").cloned()),
        Some(Some(json!("local-sibling"))),
        "the sibling pool's candidates must be unchanged: {sibling}"
    );
    assert_eq!(
        on_disk.get("model_pool_reserved_models"),
        Some(&json!(["retired-model"])),
        "the retired alias list must be unchanged: {on_disk}"
    );
    let providers = on_disk
        .get("providers")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("the migrated configuration must keep its providers: {on_disk}"));
    assert_eq!(providers.len(), 2, "both providers must survive: {on_disk}");
    assert_eq!(
        providers[1]
            .get("keys")
            .and_then(Value::as_array)
            .map(|keys| keys[0].get("value").cloned()),
        Some(Some(json!("sk-secret-b"))),
        "the provider credentials must be unchanged: {on_disk}"
    );
    assert_eq!(
        on_disk
            .get("model_prices")
            .and_then(Value::as_array)
            .map(|rows| rows[0].get("input").cloned()),
        Some(Some(json!(1.5))),
        "the price table must be unchanged: {on_disk}"
    );
}

/// REQ-005 / AC-007: the migration runs exactly once. A second read of the
/// migrated file neither rewrites it nor changes the migrated list.
#[test]
fn legacy_pool_fallback_migration_is_stamped_once_and_a_second_read_does_not_rewrite() {
    let _home = isolated_temp_home("migration-legacy-pool-fallback-once");
    write_encrypted_config(&legacy_pool_fallback_config());

    read_config().expect("the first read must migrate the legacy configuration");
    let bytes_after_migration = raw_config_bytes();
    let migrated_once = decrypted_config_json();
    assert_eq!(
        persisted_fallback_models(&migrated_once, "pool-main"),
        vec!["local-fallback".to_string()],
        "the first read must migrate the legacy fallback: {migrated_once}"
    );

    read_config().expect("the second read must stay readable");
    assert_eq!(
        raw_config_bytes(),
        bytes_after_migration,
        "a second read must not rewrite the migrated configuration"
    );
    let migrated_twice = decrypted_config_json();
    assert_eq!(
        persisted_fallback_models(&migrated_twice, "pool-main"),
        vec!["local-fallback".to_string()],
        "a second read must keep the migrated list: {migrated_twice}"
    );
    assert_eq!(
        migrated_twice.get("schema_version").and_then(Value::as_u64),
        Some(u64::from(ORDERED_FALLBACK_SCHEMA_VERSION)),
        "the migrated file must stay stamped with schema version 4: {migrated_twice}"
    );
}

/// REQ-001 / AC-007: a current-version configuration with a missing or null
/// fallback value reads as an empty list and is never rewritten, so a
/// current-schema file keeps its exact bytes.
#[test]
fn current_version_pool_without_a_fallback_value_is_not_rewritten() {
    let _home = isolated_temp_home("migration-current-version-pool");
    let mut config = legacy_pool_fallback_config();
    config["schema_version"] =
        json!(crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION);
    // The current-version file carries no legacy fallback at all, and its
    // sibling pool carries an explicit null.
    let pools = config["model_pools"].as_array_mut().expect("model pools");
    let main = pools
        .iter_mut()
        .find(|pool| pool["id"] == "pool-main")
        .expect("the main pool");
    main.as_object_mut()
        .expect("the main pool object")
        .remove("fallback");
    let sibling = pools
        .iter_mut()
        .find(|pool| pool["id"] == "pool-sibling")
        .expect("the sibling pool");
    sibling["fallback"] = json!(null);
    write_encrypted_config(&config);
    let bytes_before = raw_config_bytes();

    let read = read_config().expect("the current-version configuration must stay readable");
    assert_eq!(
        read.schema_version,
        crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION,
        "a current-version configuration must keep its version"
    );
    assert_eq!(
        raw_config_bytes(),
        bytes_before,
        "a current-version configuration must never be rewritten"
    );
}

#[test]
fn opencode_zen_sidecar_schema5_migration_red() {
    let _home = isolated_temp_home("migration-schema5-opencode-zen");
    let cfg = json!({
        "schema_version": 4,
        "enabled": true,
        "providers": [
            {
                "id": "target",
                "name": "OpenCode Zen Free",
                "base_url": "https://opencode.ai/zen/v1",
                "api_key": "sk-target",
                "protocol": "chat_completions",
                "mappings": [{"local_model": "zen-free-glm", "upstream_model": "glm-5.3-flash-free", "enabled": true}],
                "keys": [{"id": "k", "name": "Default", "value": "sk-target"}]
            },
            {
                "id": "other",
                "name": "Other",
                "base_url": "https://other.example.com/v1",
                "api_key": "sk-other",
                "protocol": "chat_completions",
                "mappings": []
            }
        ]
    });
    write_encrypted_config(&cfg);
    let pre_bytes = raw_config_bytes();
    let backup_path = config_path().unwrap().parent().unwrap().join("ai_gateway.json.bak-pre-schema5");
    let _ = fs::remove_file(&backup_path);

    read_config().expect("first read must migrate");
    let on_disk = decrypted_config_json();
    let providers = on_disk["providers"].as_array().expect("providers");
    let target = providers.iter().find(|p| p["id"] == "target").expect("target");
    assert_eq!(target["template_id"], "opencode-zen-free");
    assert_eq!(target["mappings"].as_array().unwrap().len(), 1);
    let other = providers.iter().find(|p| p["id"] == "other").expect("other");
    assert!(other.get("template_id").is_none() || other["template_id"].is_null());
    assert_eq!(on_disk["schema_version"], crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION);

    let bytes_after = raw_config_bytes();
    read_config().expect("second read");
    assert_eq!(raw_config_bytes(), bytes_after);

    assert!(backup_path.exists());
    let backup_bytes = fs::read(&backup_path).expect("backup bytes");
    assert_eq!(backup_bytes, pre_bytes);
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(&backup_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);

    // second read does not overwrite backup
    assert_eq!(fs::read(&backup_path).unwrap(), pre_bytes);
}

#[test]
fn opencode_zen_sidecar_schema5_counterexamples_red() {
    let cases = [
        ("Other Name", "https://opencode.ai/zen/v1", None),
        ("OpenCode Zen Free", "https://example.com/zen/v1", None),
        ("OpenCode Zen Free", "https://opencode.ai/zen/v1", Some("existing-template")),
    ];
    for (name, url, tid) in cases {
        let _home = isolated_temp_home("migration-schema5-counterexample");
        let mut provider = json!({
            "id": "target",
            "name": name,
            "base_url": url,
            "api_key": "sk-target",
            "protocol": "chat_completions",
            "mappings": []
        });
        if let Some(t) = tid {
            provider["template_id"] = json!(t);
        }
        let cfg = json!({
            "schema_version": 4,
            "enabled": true,
            "providers": [provider]
        });
        write_encrypted_config(&cfg);
        read_config().expect("read");
        let on_disk = decrypted_config_json();
        let target = &on_disk["providers"][0];
        if tid.is_some() {
            assert_eq!(target["template_id"], tid.unwrap());
        } else {
            assert!(target.get("template_id").is_none() || target["template_id"].is_null());
        }
    }
}

/// A saved built-in OpenCode Zen Free template snapshot that still carries the
/// legacy direct transport must be repaired to the sidecar transport on read,
/// while unrelated custom templates stay direct.
#[test]
fn zen_free_template_transport_repaired_on_read() {
    for (schema, transport) in [(4u32, None), (5u32, Some("direct")), (6u32, Some("direct"))] {
        let _home = isolated_temp_home(&format!("migration-zen-free-transport-{schema}"));
        let mut template = json!({
            "id": "opencode-zen-free",
            "name": "OpenCode Zen Free",
            "base_url": "https://opencode.ai/zen/v1",
            "protocol": "chat_completions",
            "source": "https://opencode.ai/zen/v1/models",
            "models_url": "https://opencode.ai/zen/v1/models",
            "models": [{"upstream_model": "glm-5.3-flash-free", "protocol": "chat_completions"}]
        });
        if let Some(value) = transport {
            template["transport_strategy"] = json!(value);
        }
        let cfg = json!({
            "schema_version": schema,
            "enabled": true,
            "providers": [
                {
                    "id": "bound",
                    "name": "OpenCode Zen Free",
                    "base_url": "https://opencode.ai/zen/v1",
                    "protocol": "chat_completions",
                    "template_id": "opencode-zen-free",
                    "enabled": true,
                    "keys": [{"id": "k", "name": "Default", "value": "SAFE_FIXTURE_sk-bound", "enabled": true, "auto_marked": false, "failure_kind": null, "marked_at": null, "reason": null}],
                    "mappings": [{"local_model": "zen-free-glm", "upstream_model": "glm-5.3-flash-free", "enabled": true}]
                },
                {
                    "id": "unrelated",
                    "name": "Custom Direct",
                    "base_url": "https://opencode.ai/zen/v1",
                    "protocol": "chat_completions",
                    "template_id": "custom-direct",
                    "mappings": []
                }
            ],
            "model_prices": [
                {"provider_id": "bound", "upstream_model": "glm-5.3-flash-free", "input": 1.0}
            ],
            "provider_templates": [
                {
                    "template_id": "opencode-zen-free",
                    "template": template,
                    "synced_at": 123,
                    "source": "https://opencode.ai/zen/v1/models"
                },
                {
                    "template_id": "custom-direct",
                    "template": {
                        "id": "custom-direct",
                        "name": "Custom Direct",
                        "base_url": "https://opencode.ai/zen/v1",
                        "protocol": "chat_completions",
                        "transport_strategy": "direct",
                        "models": []
                    },
                    "synced_at": 456,
                    "source": "https://example.com/models"
                }
            ]
        });
        write_encrypted_config(&cfg);
        let before = raw_config_bytes();

        let loaded = read_config().expect("older config must migrate and read");
        assert_eq!(
            loaded.schema_version,
            crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION,
            "migrated config must reach the current schema"
        );

        let zen_state = loaded
            .provider_templates
            .iter()
            .find(|state| state.template_id == "opencode-zen-free")
            .expect("the Zen Free template state must survive");
        let zen_template = zen_state.template.as_ref().expect("Zen Free snapshot");
        let zen_json = serde_json::to_value(zen_template).expect("serialize template");
        assert_eq!(
            zen_json["transport_strategy"], "opencode_zen_sidecar",
            "the repaired Zen Free snapshot must use the sidecar transport"
        );
        assert_eq!(
            zen_json["models_url"], "https://opencode.ai/zen/v1/models",
            "the existing canonical models_url on the Zen Free snapshot must survive"
        );

        let custom_state = loaded
            .provider_templates
            .iter()
            .find(|state| state.template_id == "custom-direct")
            .expect("the unrelated custom template must survive");
        let custom_json = serde_json::to_value(custom_state.template.as_ref().unwrap()).expect("serialize custom");
        assert_eq!(
            custom_json["transport_strategy"], "direct",
            "an unrelated custom template must stay direct even on the same URL"
        );

        // Provider, mappings, keys and prices survive.
        let bound = loaded.providers.iter().find(|p| p.id == "bound").expect("bound provider");
        assert_eq!(bound.transport, crate::ai_gateway::TransportStrategy::OpencodeZenSidecar);
        assert_eq!(bound.mappings.len(), 1);
        assert_eq!(bound.mappings[0].local_model, "zen-free-glm");
        assert_eq!(bound.keys.len(), 1);
        assert_eq!(bound.keys[0].value, "SAFE_FIXTURE_sk-bound");
        assert!(bound.enabled);
        assert_eq!(loaded.model_prices.len(), 1);

        let custom = loaded
            .providers
            .iter()
            .find(|p| p.id == "unrelated")
            .expect("custom-direct bound provider");
        assert_eq!(custom.transport, crate::ai_gateway::TransportStrategy::Direct);

        let after = raw_config_bytes();
        assert_ne!(after, before, "an older config must persist repaired bytes");

        // Second read is side-effect free.
        let stable = raw_config_bytes();
        let _second = read_config().expect("second read");
        assert_eq!(raw_config_bytes(), stable, "a second read must not rewrite");
    }

    // Current and future versions must not rerun the migration or rewrite bytes.
    for schema in [
        crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION,
        99u32,
    ] {
        let _home = isolated_temp_home(&format!("migration-zen-free-transport-current-{schema}"));
        let cfg = json!({
            "schema_version": schema,
            "enabled": true,
            "providers": [],
            "provider_templates": [
                {
                    "template_id": "opencode-zen-free",
                    "template": {
                        "id": "opencode-zen-free",
                        "name": "OpenCode Zen Free",
                        "base_url": "https://opencode.ai/zen/v1",
                        "protocol": "chat_completions",
                        "transport_strategy": "direct",
                        "models": []
                    },
                    "synced_at": 123,
                    "source": "https://opencode.ai/zen/v1/models"
                }
            ]
        });
        write_encrypted_config(&cfg);
        let before = raw_config_bytes();
        let _loaded = read_config().expect("current/future config must stay readable");
        assert_eq!(
            raw_config_bytes(),
            before,
            "a current or future version must not be rewritten"
        );
        let on_disk = decrypted_config_json();
        let state = on_disk["provider_templates"][0]["template"].clone();
        assert_eq!(
            state["transport_strategy"], "direct",
            "a current or future version must not rerun the transport migration"
        );
    }
}

#[test]
fn zen_free_model_list_schema8_repairs_models_url() {
    // Older schemas repair transport + models_url as applicable; custom template untouched.
    for (schema, transport) in [(5u32, Some("direct")), (6u32, Some("direct")), (7u32, Some("opencode_zen_sidecar"))] {
        let _home = isolated_temp_home(&format!("migration-zen-free-models-url-{schema}"));
        let mut template = json!({
            "id": "opencode-zen-free",
            "name": "OpenCode Zen Free",
            "base_url": "https://opencode.ai/zen/v1",
            "protocol": "chat_completions",
            "source": "https://opencode.ai/zen/v1/models",
            "models": [{"upstream_model": "glm-5.3-flash-free", "protocol": "chat_completions"}]
        });
        if let Some(value) = transport {
            template["transport_strategy"] = json!(value);
        }
        // models_url intentionally omitted (null/blank repair case)
        let cfg = json!({
            "schema_version": schema,
            "enabled": true,
            "providers": [
                {
                    "id": "bound",
                    "name": "OpenCode Zen Free",
                    "base_url": "https://opencode.ai/zen/v1",
                    "protocol": "chat_completions",
                    "template_id": "opencode-zen-free",
                    "enabled": true,
                    "keys": [{"id": "k", "name": "Default", "value": "SAFE_FIXTURE_sk-bound", "enabled": true, "auto_marked": false, "failure_kind": null, "marked_at": null, "reason": null}],
                    "mappings": [{"local_model": "zen-free-glm", "upstream_model": "glm-5.3-flash-free", "enabled": true}]
                },
                {
                    "id": "unrelated",
                    "name": "Custom Direct",
                    "base_url": "https://opencode.ai/zen/v1",
                    "protocol": "chat_completions",
                    "template_id": "custom-direct",
                    "mappings": []
                }
            ],
            "model_prices": [
                {"provider_id": "bound", "upstream_model": "glm-5.3-flash-free", "input": 1.0}
            ],
            "provider_templates": [
                {
                    "template_id": "opencode-zen-free",
                    "template": template,
                    "synced_at": 123,
                    "source": "https://opencode.ai/zen/v1/models"
                },
                {
                    "template_id": "custom-direct",
                    "template": {
                        "id": "custom-direct",
                        "name": "Custom Direct",
                        "base_url": "https://opencode.ai/zen/v1",
                        "protocol": "chat_completions",
                        "transport_strategy": "direct",
                        "models_url": "https://custom.example.com/models",
                        "models": []
                    },
                    "synced_at": 456,
                    "source": "https://example.com/models"
                }
            ]
        });
        write_encrypted_config(&cfg);
        let before = raw_config_bytes();

        let loaded = read_config().expect("older config must migrate and read");
        assert_eq!(
            loaded.schema_version,
            crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION,
            "migrated config must reach the current schema"
        );

        let zen_state = loaded
            .provider_templates
            .iter()
            .find(|state| state.template_id == "opencode-zen-free")
            .expect("the Zen Free template state must survive");
        let zen_template = zen_state.template.as_ref().expect("Zen Free snapshot");
        let zen_json = serde_json::to_value(zen_template).expect("serialize template");
        assert_eq!(
            zen_json["models_url"], "https://opencode.ai/zen/v1/models",
            "repaired Zen Free snapshot must carry canonical models_url"
        );

        let custom_state = loaded
            .provider_templates
            .iter()
            .find(|state| state.template_id == "custom-direct")
            .expect("the unrelated custom template must survive");
        let custom_json = serde_json::to_value(custom_state.template.as_ref().unwrap()).expect("serialize custom");
        assert_eq!(
            custom_json["models_url"], "https://custom.example.com/models",
            "an unrelated custom template models_url must stay unchanged"
        );

        // data preserved
        let bound = loaded.providers.iter().find(|p| p.id == "bound").expect("bound provider");
        assert_eq!(bound.mappings.len(), 1);
        assert_eq!(bound.mappings[0].local_model, "zen-free-glm");
        assert_eq!(bound.keys.len(), 1);
        assert_eq!(bound.keys[0].value, "SAFE_FIXTURE_sk-bound");
        assert!(bound.enabled);
        assert_eq!(loaded.model_prices.len(), 1);

        let after = raw_config_bytes();
        assert_ne!(after, before, "an older config must persist repaired bytes");

        // second read no rewrite
        let stable = raw_config_bytes();
        let _second = read_config().expect("second read");
        assert_eq!(raw_config_bytes(), stable, "a second read must not rewrite");
    }

    // Current and future versions untouched.
    for schema in [crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION, 99u32] {
        let _home = isolated_temp_home(&format!("migration-zen-free-models-url-current-{schema}"));
        let cfg = json!({
            "schema_version": schema,
            "enabled": true,
            "providers": [],
            "provider_templates": [
                {
                    "template_id": "opencode-zen-free",
                    "template": {
                        "id": "opencode-zen-free",
                        "name": "OpenCode Zen Free",
                        "base_url": "https://opencode.ai/zen/v1",
                        "protocol": "chat_completions",
                        "transport_strategy": "direct",
                        "models": []
                    },
                    "synced_at": 123,
                    "source": "https://opencode.ai/zen/v1/models"
                }
            ]
        });
        write_encrypted_config(&cfg);
        let before = raw_config_bytes();
        let _loaded = read_config().expect("current/future config must stay readable");
        assert_eq!(raw_config_bytes(), before, "a current or future version must not be rewritten");
        let on_disk = decrypted_config_json();
        let state = on_disk["provider_templates"][0]["template"].clone();
        assert!(state.get("models_url").is_none() || state["models_url"].is_null(),
            "a current or future version must not repair models_url");
    }
}

#[test]
fn zen_free_inference_endpoint_schema9_migrates_base_url() {
    for schema in [5u32, 6, 7, 8] {
        let _home = isolated_temp_home(&format!("migration-zen-free-inference-{schema}"));
        let cfg = json!({
            "schema_version": schema,
            "enabled": true,
            "providers": [
                {
                    "id": "bound",
                    "name": "OpenCode Zen Free",
                    "base_url": "https://opencode.ai/zen/v1",
                    "protocol": "chat_completions",
                    "template_id": "opencode-zen-free",
                    "enabled": true,
                    "keys": [{"id": "k", "name": "Default", "value": "SAFE_FIXTURE_sk-bound", "enabled": true, "auto_marked": false, "failure_kind": null, "marked_at": null, "reason": null}],
                    "mappings": [{"local_model": "zen-free-glm", "upstream_model": "glm-5.3-flash-free", "enabled": true}],
                    "transport": "opencode_zen_sidecar"
                },
                {
                    "id": "custom",
                    "name": "Custom Direct",
                    "base_url": "https://custom.example.com/v1",
                    "protocol": "chat_completions",
                    "template_id": "custom-direct",
                    "mappings": []
                }
            ],
            "model_prices": [
                {"provider_id": "bound", "upstream_model": "glm-5.3-flash-free", "input": 1.0}
            ],
            "provider_templates": [
                {
                    "template_id": "opencode-zen-free",
                    "template": {
                        "id": "opencode-zen-free",
                        "name": "OpenCode Zen Free",
                        "base_url": "https://opencode.ai/zen/v1",
                        "protocol": "chat_completions",
                        "source": "https://opencode.ai/zen/v1/models",
                        "models_url": "https://opencode.ai/zen/v1/models",
                        "transport_strategy": "opencode_zen_sidecar",
                        "models": [{"upstream_model": "glm-5.3-flash-free", "protocol": "chat_completions"}]
                    },
                    "synced_at": 123,
                    "source": "https://opencode.ai/zen/v1/models"
                },
                {
                    "template_id": "custom-direct",
                    "template": {
                        "id": "custom-direct",
                        "name": "Custom Direct",
                        "base_url": "https://custom.example.com/v1",
                        "protocol": "chat_completions",
                        "transport_strategy": "direct",
                        "models_url": "https://custom.example.com/v1/models",
                        "models": []
                    },
                    "synced_at": 456,
                    "source": "https://custom.example.com/v1/models"
                }
            ]
        });
        write_encrypted_config(&cfg);
        let before = raw_config_bytes();

        let loaded = read_config().expect("older config must migrate and read");
        assert_eq!(
            loaded.schema_version,
            crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION,
            "migrated config must reach the current schema"
        );

        let zen_state = loaded
            .provider_templates
            .iter()
            .find(|state| state.template_id == "opencode-zen-free")
            .expect("the Zen Free template state must survive");
        let zen_template = zen_state.template.as_ref().expect("Zen Free snapshot");
        let zen_json = serde_json::to_value(zen_template).expect("serialize template");
        assert_eq!(
            zen_json["base_url"], "https://opencode.ai/inference/openai/v1",
            "the migrated Zen Free template base_url must use the inference endpoint"
        );
        assert_eq!(
            zen_json["models_url"], "https://opencode.ai/zen/v1/models",
            "the existing canonical models_url on the Zen Free snapshot must survive"
        );

        let bound = loaded.providers.iter().find(|p| p.id == "bound").expect("bound provider");
        assert_eq!(bound.base_url, "https://opencode.ai/inference/openai/v1");
        assert_eq!(bound.mappings.len(), 1);
        assert_eq!(bound.keys.len(), 1);
        assert_eq!(bound.keys[0].value, "SAFE_FIXTURE_sk-bound");

        let custom = loaded
            .providers
            .iter()
            .find(|p| p.id == "custom")
            .expect("custom provider");
        assert_eq!(custom.base_url, "https://custom.example.com/v1");

        let custom_state = loaded
            .provider_templates
            .iter()
            .find(|state| state.template_id == "custom-direct")
            .expect("the unrelated custom template must survive");
        let custom_json = serde_json::to_value(custom_state.template.as_ref().unwrap()).expect("serialize custom");
        assert_eq!(custom_json["base_url"], "https://custom.example.com/v1");

        let after = raw_config_bytes();
        assert_ne!(after, before, "an older config must persist migrated bytes");

        // Second read is side-effect free.
        let stable = raw_config_bytes();
        let _second = read_config().expect("second read");
        assert_eq!(raw_config_bytes(), stable, "a second read must not rewrite");
    }

    // Current and future versions must not rerun the migration or rewrite bytes.
    for schema in [
        crate::ai_gateway::GATEWAY_CONFIG_SCHEMA_VERSION,
        99u32,
    ] {
        let _home = isolated_temp_home(&format!("migration-zen-free-inference-current-{schema}"));
        let cfg = json!({
            "schema_version": schema,
            "enabled": true,
            "providers": [
                {
                    "id": "bound",
                    "name": "OpenCode Zen Free",
                    "base_url": "https://opencode.ai/zen/v1",
                    "protocol": "chat_completions",
                    "template_id": "opencode-zen-free",
                    "mappings": []
                }
            ],
            "provider_templates": [
                {
                    "template_id": "opencode-zen-free",
                    "template": {
                        "id": "opencode-zen-free",
                        "name": "OpenCode Zen Free",
                        "base_url": "https://opencode.ai/zen/v1",
                        "protocol": "chat_completions",
                        "transport_strategy": "opencode_zen_sidecar",
                        "models": []
                    },
                    "synced_at": 123,
                    "source": "https://opencode.ai/zen/v1/models"
                }
            ]
        });
        write_encrypted_config(&cfg);
        let before = raw_config_bytes();
        let _loaded = read_config().expect("current/future config must stay readable");
        assert_eq!(
            raw_config_bytes(),
            before,
            "a current or future version must not be rewritten"
        );
        let on_disk = decrypted_config_json();
        let state = on_disk["provider_templates"][0]["template"].clone();
        assert_eq!(
            state["base_url"], "https://opencode.ai/zen/v1",
            "a current or future version must not rerun the base_url migration"
        );
    }
}
