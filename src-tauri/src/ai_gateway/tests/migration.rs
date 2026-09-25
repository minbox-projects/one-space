//! Behavior tests for the version-gated one-time legacy migration.
//!
//! Every fixture is built from raw JSON (configuration) or raw SQL (usage
//! database) so this module compiles against both the current API and the
//! cleaned API, and fails behaviorally — never because a future symbol is
//! missing. The configuration is read through [`super::super::storage::read_config`],
//! the only public read boundary, and on-disk shapes are inspected as
//! `serde_json::Value` plus raw bytes.

use super::{config_path, isolated_temp_home};
use crate::ai_gateway::storage::read_config;
use crate::ai_gateway::{LogFilter, ModelPrice, TimeRange, UsageLogStore};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

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
        "schema_version": 2,
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

/// Boundary: a configuration at a future/unknown schema version (here `2`) is
/// read as-is. There is no downgrade, no rewrite and no deletion: values are
/// preserved in memory — including a global price row the current migration
/// would otherwise scope — and the raw file bytes stay byte-identical after the
/// read. A regression that treated any non-current version as migratable would
/// rewrite the bytes and scope the row.
#[test]
fn future_schema_version_config_is_read_without_downgrade_or_rewrite() {
    let _home = isolated_temp_home("migration-future-version");
    let fixture = json!({
        "schema_version": 2,
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
        loaded.schema_version, 2,
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

// ---------------------------------------------------------------------------
// Ordered named key pool migration (REQ-002 / AC-002)
//
// The key-pool fields are not on the typed provider yet, so the migrated shape
// is asserted on the decrypted raw JSON. The fixtures carry the legacy
// `api_key` field; the expected post-migration shape is schema version 2 with a
// `keys` array and no `api_key`.
// ---------------------------------------------------------------------------

/// Encoded current key-pool schema version.
const KEY_POOL_SCHEMA_VERSION: u32 = 2;

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
