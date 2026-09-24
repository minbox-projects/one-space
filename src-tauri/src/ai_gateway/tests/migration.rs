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
        "schema_version": 1,
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

    let home = isolated_temp_home("migration-readonly-retry");
    write_encrypted_config(&legacy_config_fixture());
    let legacy_bytes = raw_config_bytes();

    let dir = home.path.clone();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o500))
        .expect("make the config directory read-only");
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
