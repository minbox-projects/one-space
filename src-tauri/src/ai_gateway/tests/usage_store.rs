//! REQ-005/AC-005 behavior tests for transactional, path-scoped gateway usage
//! logging (plan Step 5).
//!
//! These exercise the public append paths of [`UsageLogStore`] against real
//! temporary SQLite databases. The only test seam used is the documented
//! one-shot insert fault (`inject_usage_log_insert_failure_at`); no other
//! internal mocking is involved.
//!
//! The append paths also increment process-global benchmark counters
//! ([`usage_log_write_stats`]) that every other test in this binary touches
//! while running in parallel, so exact counter equality cannot be asserted from
//! a parallel behavior test. The behavior tests below assert the observable
//! row-level contract (rollback with no partial rows, per-path isolation,
//! exactly one terminal row, failure isolation); the exact transaction/open
//! counter contract is asserted by the serialized ignored
//! `core_workflows_perf_usage_log_atomic_batch` test, which the measurement
//! harness runs with `--test-threads=1`.

use crate::ai_gateway::usage_log::{
    clear_usage_log_insert_failure, inject_usage_log_insert_failure_at, reset_usage_log_write_stats,
    usage_log_write_stats, UsageAccounting, UsageLogEntry, UsageLogRecord, UsageLogStore,
    UsageLogWriteStats, UsageResult,
};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

/// The one-shot fault is process-global, so the tests that arm it must not run
/// concurrently with each other.
static SEAM_LOCK: Mutex<()> = Mutex::new(());

/// Index of the entry whose insert is injected to fail. Chosen beyond any batch
/// length used elsewhere in this test binary (the largest is 6), so a parallel
/// test's smaller batch can never consume this one-shot fault first.
const INJECTED_INDEX: usize = 8;
/// Length of the batch armed for the injected failure: strictly longer than
/// [`INJECTED_INDEX`] so the fault is a mid-batch failure.
const SEALED_BATCH_LEN: usize = 16;

fn lock_seam() -> MutexGuard<'static, ()> {
    SEAM_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn temp_db(name: &str) -> (PathBuf, PathBuf) {
    let dir = super::make_temp_dir(name);
    fs::create_dir_all(&dir).expect("create temp dir");
    let db_path = dir.join("ai_gateway_usage.db");
    (dir, db_path)
}

fn subtract(after: UsageLogWriteStats, before: UsageLogWriteStats) -> UsageLogWriteStats {
    UsageLogWriteStats {
        db_opens: after.db_opens.wrapping_sub(before.db_opens),
        transactions: after.transactions.wrapping_sub(before.transactions),
        rows_written: after.rows_written.wrapping_sub(before.rows_written),
        batch_writes: after.batch_writes.wrapping_sub(before.batch_writes),
    }
}

fn record(local_model: &str, index: usize, terminal: bool) -> UsageLogRecord {
    super::sample_attempt_record(
        chrono::Utc::now().timestamp_millis() + index as i64,
        local_model,
        "remote-a",
        "p1",
        "Provider One",
        UsageResult::Success,
        terminal,
        None,
        Some(0.1),
        super::tokens(4, 1, 0, 2),
    )
}

/// A batch whose last row is the request's single terminal row.
fn batch(local_model: &str, count: usize) -> Vec<UsageLogRecord> {
    (0..count)
        .map(|index| record(local_model, index, index + 1 == count))
        .collect()
}

fn entry(record: UsageLogRecord) -> UsageLogEntry {
    UsageLogEntry {
        record,
        accounting: UsageAccounting::CANONICAL,
    }
}

/// REQ-005/AC-005: an injected mid-batch insert failure makes
/// `append_batch_with_accounting` return an error and rolls the whole batch
/// back. No row of the request remains, and clearing the seam lets the same
/// batch commit exactly once.
#[test]
fn append_batch_with_accounting_rolls_back_injected_failure() {
    let _seam = lock_seam();
    let (dir, db_path) = temp_db("usage-atomic-injected-failure");
    let store = UsageLogStore::at(&db_path);

    let baseline_count = store.count().expect("baseline count");
    let entries: Vec<UsageLogEntry> = batch("atomic-fail", SEALED_BATCH_LEN)
        .into_iter()
        .map(entry)
        .collect();
    assert!(entries.len() > INJECTED_INDEX, "seam must fire mid-batch");

    inject_usage_log_insert_failure_at(INJECTED_INDEX);
    let error = store
        .append_batch_with_accounting(&entries, 365)
        .expect_err("the injected failure must return an error");
    clear_usage_log_insert_failure();
    assert!(!error.is_empty(), "the failure must carry a message");

    // No partial batch remains and the pre-existing rows are untouched.
    assert_eq!(
        store.count().expect("count after failure"),
        baseline_count,
        "a failed batch must leave no rows behind"
    );
    assert!(
        store
            .all_records()
            .expect("records after failure")
            .iter()
            .all(|row| row.local_model != "atomic-fail"),
        "no row of the failed batch may be stored"
    );

    // Clearing the seam lets the same batch commit as one unit.
    store
        .append_batch_with_accounting(&entries, 365)
        .expect("a cleared seam must let the batch commit");
    let stored = store.all_records().expect("records after success");
    assert_eq!(
        stored.len() as u32,
        baseline_count + entries.len() as u32,
        "the retried batch must commit every entry exactly once"
    );
    assert_eq!(
        stored
            .iter()
            .filter(|row| row.local_model == "atomic-fail")
            .count(),
        entries.len(),
        "the committed batch must contain all of its rows"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// REQ-005/AC-005: distinct database paths stay isolated. Two stores at
/// different paths each read back only their own rows, including after
/// repeated batch writes.
#[test]
fn distinct_database_paths_keep_their_rows_isolated() {
    let (dir_a, db_a) = temp_db("usage-isolation-a");
    let (dir_b, db_b) = temp_db("usage-isolation-b");
    let store_a = UsageLogStore::at(&db_a);
    let store_b = UsageLogStore::at(&db_b);

    store_a
        .append_batch(&batch("iso-a", 2), 365)
        .expect("first batch on path A");
    store_a
        .append_batch(&batch("iso-a", 2), 365)
        .expect("second batch on path A");
    store_b
        .append_batch(&batch("iso-b", 3), 365)
        .expect("first batch on path B");

    let records_a = store_a.all_records().expect("path A records");
    let records_b = store_b.all_records().expect("path B records");
    assert_eq!(records_a.len(), 4, "path A holds only its own four rows");
    assert_eq!(records_b.len(), 3, "path B holds only its own three rows");
    assert!(
        records_a.iter().all(|row| row.local_model == "iso-a"),
        "path A must not see path B rows"
    );
    assert!(
        records_b.iter().all(|row| row.local_model == "iso-b"),
        "path B must not see path A rows"
    );

    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}

/// REQ-005/AC-005: one request's batch stores every attempt row and exactly one
/// terminal row, all through the accounting-aware append path.
#[test]
fn one_request_batch_stores_all_attempts_and_exactly_one_terminal_row() {
    let (dir, db_path) = temp_db("usage-one-request-batch");
    let store = UsageLogStore::at(&db_path);
    let entries = vec![
        entry(record("txn", 0, false)),
        entry(record("txn", 1, false)),
        entry(record("txn", 2, true)),
    ];

    store
        .append_batch_with_accounting(&entries, 365)
        .expect("append one request batch");

    let stored = store.all_records().expect("stored records");
    assert_eq!(stored.len(), 3, "every attempt row must be stored");
    assert_eq!(
        stored.iter().filter(|row| row.terminal).count(),
        1,
        "a request must store exactly one terminal row"
    );
    assert_eq!(
        stored.iter().filter(|row| !row.terminal).count(),
        2,
        "the batch must keep its non-terminal attempt rows"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// REQ-005/AC-005: a failing usage-log batch does not panic or corrupt the
/// store, and later successful appends still work.
#[test]
fn logging_failure_is_isolated_from_later_successful_appends() {
    let _seam = lock_seam();
    let (dir, db_path) = temp_db("usage-failure-isolation");
    let store = UsageLogStore::at(&db_path);

    store
        .append_batch(&batch("iso-good", 3), 365)
        .expect("baseline append");

    let failing: Vec<UsageLogRecord> = batch("iso-fail", SEALED_BATCH_LEN);
    inject_usage_log_insert_failure_at(INJECTED_INDEX);
    let error = store
        .append_batch(&failing, 365)
        .expect_err("the injected failure must return an error");
    clear_usage_log_insert_failure();
    assert!(!error.is_empty(), "the failure must carry a message");

    // The failure did not panic and left the earlier rows intact.
    let after_failure = store.all_records().expect("records after failure");
    assert_eq!(after_failure.len(), 3, "the failed batch added no rows");
    assert!(
        after_failure
            .iter()
            .all(|row| row.local_model == "iso-good"),
        "the failed batch must not appear"
    );

    // A subsequent append still works and commits.
    store
        .append_batch(&batch("iso-later", 2), 365)
        .expect("a later append must succeed");
    let records = store.all_records().expect("records after later append");
    assert_eq!(records.len(), 5, "the later append must commit both rows");
    assert_eq!(
        records
            .iter()
            .filter(|row| row.local_model == "iso-later")
            .count(),
        2,
        "the later rows must be present"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// REQ-005/AC-005 exact counter contract: one request's whole batch is exactly
/// one committed transaction (and one batch write) across all of its attempts,
/// a reused path opens one connection, distinct paths open their own
/// connections, and an injected mid-batch failure leaves the counters and the
/// committed rows untouched.
///
/// The write counters are process-global, so this test requires the harness's
/// serialized run `cargo test ... core_workflows_perf -- --ignored
/// --nocapture --test-threads=1`. It prints no `CWF_METRIC` line.
#[test]
#[ignore = "serialized REQ-005/AC-005 counter contract; run with --test-threads=1"]
fn core_workflows_perf_usage_log_atomic_batch() {
    let _seam = lock_seam();
    reset_usage_log_write_stats();

    // One request's batch = exactly one committed transaction across attempts.
    let (dir_one, db_one) = temp_db("usage-counter-one-request");
    let store_one = UsageLogStore::at(&db_one);
    store_one
        .append_batch(&batch("counter-one", 3), 365)
        .expect("append one request batch");
    let after_first = usage_log_write_stats();
    assert_eq!(
        after_first,
        UsageLogWriteStats {
            db_opens: 1,
            transactions: 1,
            rows_written: 3,
            batch_writes: 1,
        },
        "one three-attempt request must be one committed transaction"
    );

    // A second batch reuses the same path's connection (no new open).
    store_one
        .append_batch(&batch("counter-one", 2), 365)
        .expect("append a second request batch");
    let after_reuse = usage_log_write_stats();
    assert_eq!(
        after_reuse,
        UsageLogWriteStats {
            db_opens: 1,
            transactions: 2,
            rows_written: 5,
            batch_writes: 2,
        },
        "a repeated path must reuse its connection and commit once per batch"
    );

    // A distinct path opens its own connection and stays isolated.
    let (dir_two, db_two) = temp_db("usage-counter-distinct-path");
    let store_two = UsageLogStore::at(&db_two);
    store_two
        .append_batch(&batch("counter-two", 2), 365)
        .expect("append on a second path");
    let after_second_path = usage_log_write_stats();
    assert_eq!(
        after_second_path,
        UsageLogWriteStats {
            db_opens: 2,
            transactions: 3,
            rows_written: 7,
            batch_writes: 3,
        },
        "a second path must open its own connection"
    );
    assert!(
        store_two
            .all_records()
            .expect("second-path records")
            .iter()
            .all(|row| row.local_model == "counter-two"),
        "the second path must not see the first path's rows"
    );

    // An injected mid-batch failure moves neither counters nor rows. The
    // read above opened a query connection, so the baseline is taken here.
    let before_failure = usage_log_write_stats();
    let failing: Vec<UsageLogRecord> = batch("counter-fail", SEALED_BATCH_LEN);
    inject_usage_log_insert_failure_at(INJECTED_INDEX);
    let error = store_one
        .append_batch(&failing, 365)
        .expect_err("the injected failure must return an error");
    clear_usage_log_insert_failure();
    assert!(!error.is_empty());
    assert_eq!(
        usage_log_write_stats(),
        before_failure,
        "a failed batch must not move transactions, rows or batch writes"
    );
    assert_eq!(
        store_one.count().expect("count after failure"),
        5,
        "a failed batch must leave no partial rows"
    );

    // Clearing the seam lets the same batch commit as one unit on the reused
    // connection: one more transaction, one more batch write, no new open.
    let before_recovery = usage_log_write_stats();
    store_one
        .append_batch(&failing, 365)
        .expect("a cleared seam must let the batch commit");
    assert_eq!(
        subtract(usage_log_write_stats(), before_recovery),
        UsageLogWriteStats {
            db_opens: 0,
            transactions: 1,
            rows_written: SEALED_BATCH_LEN as u64,
            batch_writes: 1,
        },
        "the recovered batch must commit once with all of its rows"
    );
    assert_eq!(
        store_one.count().expect("count after recovery"),
        5 + SEALED_BATCH_LEN as u32
    );

    let _ = fs::remove_dir_all(&dir_one);
    let _ = fs::remove_dir_all(&dir_two);
}
