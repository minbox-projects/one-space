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
    usage_log_write_stats, LogFilter, TimeRange, UsageAccounting, UsageLogEntry, UsageLogRecord,
    UsageLogStore, UsageLogWriteStats, UsageResult,
};
use crate::ai_gateway::{
    clear_provider_usage_summary_failure, clear_usage_clock_for_test,
    inject_provider_usage_summary_failure, set_usage_clock_for_test,
};
use rusqlite::Connection;
use serde_json::Value;
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

/// REQ-004/AC-005: committed rows are immediately visible to subsequent public
/// reads, and a reader or writer bound to the same path reopens the database when
/// the file is replaced or deleted. A stale cached handle would keep returning
/// the old rows; a result cache would hide the just-committed row.
#[test]
fn query_reads_see_committed_writes_and_reopen_replaced_or_deleted_databases() {
    let (dir, db_path) = temp_db("usage-query-identity");
    let (replacement_dir, replacement_path) = temp_db("usage-query-identity-replacement");
    let store = UsageLogStore::at(&db_path);

    store
        .append_batch(&batch("query-first", 2), 365)
        .expect("first committed batch");
    let first = store
        .query_logs(&TimeRange::default(), &LogFilter::default(), 1)
        .expect("first query");
    assert_eq!(first.total, 2, "the first committed batch must be visible");
    assert!(
        first.records.iter().all(|row| row.local_model == "query-first"),
        "the first query must see only the first batch: {:?}",
        first.records
    );

    // A write followed by a read through the same path must see the new row.
    store
        .append_batch(&batch("query-second", 1), 365)
        .expect("second committed batch");
    let second = store
        .query_logs(&TimeRange::default(), &LogFilter::default(), 1)
        .expect("query after a later commit");
    assert_eq!(second.total, 3, "a later commit must be visible immediately");
    assert!(
        second
            .records
            .iter()
            .any(|row| row.local_model == "query-second"),
        "the later batch must appear in the next query: {:?}",
        second.records
    );

    // Build a different database at another real path, then replace this path.
    {
        let replacement = UsageLogStore::at(&replacement_path);
        replacement
            .append_batch(&batch("query-replacement", 4), 365)
            .expect("replacement database rows");
    }
    fs::rename(&replacement_path, &db_path).expect("replace the usage database");
    let replaced = store
        .query_logs(&TimeRange::default(), &LogFilter::default(), 1)
        .expect("query after replacing the database");
    assert_eq!(replaced.total, 4, "the replaced database must be read, not the old one");
    assert!(
        replaced
            .records
            .iter()
            .all(|row| row.local_model == "query-replacement"),
        "no row from the replaced database may survive: {:?}",
        replaced.records
    );

    // The writer must also reopen the replacement instead of writing to the old
    // handle, and the next reader must observe that committed row.
    store
        .append_batch(&batch("query-after-replace", 1), 365)
        .expect("write after replacing the database");
    let after_writer = store
        .query_logs(&TimeRange::default(), &LogFilter::default(), 1)
        .expect("query after the writer reopened");
    assert_eq!(after_writer.total, 5);
    assert_eq!(
        after_writer
            .records
            .iter()
            .filter(|row| row.local_model == "query-replacement")
            .count(),
        4,
        "the replacement rows must remain: {:?}",
        after_writer.records
    );
    assert_eq!(
        after_writer
            .records
            .iter()
            .filter(|row| row.local_model == "query-after-replace")
            .count(),
        1,
        "the writer must commit into the replacement database"
    );

    // Deleting the file invalidates both roles: a read creates a fresh empty
    // database, and a later append/read works against the new file.
    fs::remove_file(&db_path).expect("delete the usage database");
    let deleted = store
        .query_logs(&TimeRange::default(), &LogFilter::default(), 1)
        .expect("query after deleting the database");
    assert_eq!(deleted.total, 0, "a deleted database must not return old rows");
    store
        .append_batch(&batch("query-after-delete", 1), 365)
        .expect("write after deleting the database");
    let recreated = store
        .query_logs(&TimeRange::default(), &LogFilter::default(), 1)
        .expect("query after recreating the database");
    assert_eq!(recreated.total, 1);
    assert_eq!(recreated.records[0].local_model, "query-after-delete");

    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&replacement_dir);
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

// ---------------------------------------------------------------------------
// REQ-003 / REQ-004 / AC-003 / AC-004: daily-summary atomicity and retention
// ---------------------------------------------------------------------------

/// One terminal-or-attempt row for provider `provider_id` on a single UTC+8 day
/// (record timestamps are absolute, so the day is independent of the clock).
fn provider_entry(provider_id: &str, timestamp_ms: i64, terminal: bool) -> UsageLogEntry {
    UsageLogEntry {
        record: super::sample_attempt_record(
            timestamp_ms,
            "local-p",
            "remote-p",
            provider_id,
            "Provider P",
            UsageResult::Success,
            terminal,
            None,
            Some(0.5),
            super::tokens(2, 0, 0, 2),
        ),
        accounting: UsageAccounting::CANONICAL,
    }
}

fn provider_batch(provider_id: &str, timestamp_ms: i64, count: usize) -> Vec<UsageLogEntry> {
    (0..count)
        .map(|index| provider_entry(provider_id, timestamp_ms + index as i64, index + 1 == count))
        .collect()
}

/// REQ-004/AC-004: a mid-batch insert fault rolls back the request's detail rows
/// *and* its daily-summary contribution together. The provider history shows no
/// day for the failed request, and clearing the seam lets the same batch commit
/// exactly once.
#[test]
fn append_batch_rolls_back_summary_contribution_on_mid_batch_failure() {
    let _seam = lock_seam();
    set_usage_clock_for_test(super::rfc3339_millis("2026-09-20T10:00:00+08:00"));
    let (dir, db_path) = temp_db("provider-summary-mid-batch");
    let store = UsageLogStore::at(&db_path);
    let day = super::rfc3339_millis("2026-09-20T11:00:00+08:00");
    let now = super::rfc3339_millis("2026-09-20T12:00:00+08:00");
    let entries = provider_batch("txn-p", day, SEALED_BATCH_LEN);
    assert!(entries.len() > INJECTED_INDEX, "the seam must fire mid-batch");

    let before = store
        .provider_usage_history("txn-p", now)
        .expect("baseline history");
    assert!(before.days.is_empty(), "a fresh provider has no history");

    inject_usage_log_insert_failure_at(INJECTED_INDEX);
    let error = store
        .append_batch_with_accounting(&entries, 365)
        .expect_err("the injected failure must return an error");
    clear_usage_log_insert_failure();
    assert!(!error.is_empty());

    assert_eq!(store.count().expect("no detail rows"), 0);
    let after_failure = store
        .provider_usage_history("txn-p", now)
        .expect("history after failure");
    assert!(
        after_failure.days.is_empty(),
        "a failed batch must contribute no daily summary: {after_failure:?}"
    );
    assert!(after_failure.cumulative.is_none());

    // The cleared seam commits both the detail rows and the summary once.
    store
        .append_batch_with_accounting(&entries, 365)
        .expect("a cleared seam must let the batch commit");
    assert_eq!(
        store.count().expect("detail rows after retry") as usize,
        SEALED_BATCH_LEN
    );
    let after_success = store
        .provider_usage_history("txn-p", now)
        .expect("history after retry");
    assert_eq!(
        after_success.days.len(),
        1,
        "the retried batch contributes exactly one day"
    );
    assert_eq!(
        after_success
            .days[0]
            .metrics
            .as_ref()
            .map(|metrics| metrics.request_count),
        Some(1),
        "only the terminal row of the request accrues"
    );

    clear_usage_clock_for_test();
    let _ = fs::remove_dir_all(&dir);
}

/// REQ-004/AC-004: a daily-summary update failure rolls back the whole append
/// unit, leaving neither detail rows nor a summary contribution, and a later
/// successful append contributes exactly once.
#[test]
fn append_batch_rolls_back_on_summary_update_failure() {
    let _seam = lock_seam();
    set_usage_clock_for_test(super::rfc3339_millis("2026-09-20T10:00:00+08:00"));
    let (dir, db_path) = temp_db("provider-summary-fault");
    let store = UsageLogStore::at(&db_path);
    let day = super::rfc3339_millis("2026-09-20T11:00:00+08:00");
    let now = super::rfc3339_millis("2026-09-20T12:00:00+08:00");
    let entries = provider_batch("summary-p", day, 3);

    inject_provider_usage_summary_failure();
    let error = store
        .append_batch_with_accounting(&entries, 365)
        .expect_err("the injected summary failure must return an error");
    clear_provider_usage_summary_failure();
    assert!(!error.is_empty(), "the failure must carry a message");

    assert_eq!(
        store.count().expect("no partial detail rows"),
        0,
        "a summary-update failure must roll the detail rows back"
    );
    assert!(
        store
            .provider_usage_history("summary-p", now)
            .expect("history after failure")
            .days
            .is_empty(),
        "a summary-update failure must leave no contribution"
    );

    store
        .append_batch_with_accounting(&entries, 365)
        .expect("the retried append must succeed");
    assert_eq!(store.count().expect("detail rows after retry"), 3);
    let history = store
        .provider_usage_history("summary-p", now)
        .expect("history after retry");
    assert_eq!(history.days.len(), 1);
    assert_eq!(
        history.days[0]
            .metrics
            .as_ref()
            .map(|metrics| metrics.request_count),
        Some(1)
    );

    clear_usage_clock_for_test();
    let _ = fs::remove_dir_all(&dir);
}

/// REQ-004/AC-004: retention 1 and 365 delete only detail rows. Expired details
/// disappear while the persisted daily summaries and the earliest available day
/// survive, including for a row that was already outside the window when it was
/// written.
#[test]
fn retention_one_and_365_bound_detail_rows_and_keep_daily_summaries() {
    set_usage_clock_for_test(super::rfc3339_millis("2026-09-20T10:00:00+08:00"));
    let (dir, db_path) = temp_db("provider-retention-boundary");
    let store = UsageLogStore::at(&db_path);

    // Written 384 days before "now": retention 365 expires its detail row
    // immediately, but the daily summary is committed with the append unit.
    store
        .append_batch_with_accounting(
            &[provider_entry(
                "ret-p",
                super::rfc3339_millis("2025-09-01T12:00:00+08:00"),
                true,
            )],
            365,
        )
        .expect("an out-of-window append still records its summary");
    // Inside the 365-day window: detail and summary both stay.
    store
        .append_batch_with_accounting(
            &[provider_entry(
                "ret-p",
                super::rfc3339_millis("2026-09-19T05:00:00+08:00"),
                true,
            )],
            365,
        )
        .expect("an in-window append");
    assert_eq!(store.count().expect("detail rows"), 1, "only the 2025 detail expired");

    // A one-day retention deletes the 09-19 detail but keeps 09-20.
    store
        .append_batch_with_accounting(
            &[provider_entry(
                "ret-p",
                super::rfc3339_millis("2026-09-20T12:00:00+08:00"),
                true,
            )],
            1,
        )
        .expect("a one-day retention append");
    assert_eq!(store.count().expect("detail rows after 1-day retention"), 1);
    assert!(
        store
            .all_records()
            .expect("remaining details")
            .iter()
            .all(|row| row.timestamp_ms > super::rfc3339_millis("2026-09-19T10:00:00+08:00")),
        "retention 1 keeps only the current day's detail"
    );

    let history = store
        .provider_usage_history("ret-p", super::rfc3339_millis("2026-09-20T13:00:00+08:00"))
        .expect("history after retention");
    assert_eq!(
        history.earliest_available_day.as_deref(),
        Some("2025-09-01"),
        "the earliest available day survives detail expiry"
    );
    assert_eq!(history.end_day.as_deref(), Some("2026-09-20"));
    let cumulative = history.cumulative.expect("cumulative survives");
    assert_eq!(
        cumulative.request_count, 3,
        "every retained summary day still contributes"
    );
    for expected_day in ["2025-09-01", "2026-09-19", "2026-09-20"] {
        let entry = history
            .days
            .iter()
            .find(|entry| entry.day == expected_day)
            .unwrap_or_else(|| panic!("day {expected_day} missing"));
        assert_eq!(
            entry.metrics.as_ref().map(|metrics| metrics.request_count),
            Some(1),
            "day {expected_day} keeps its summary"
        );
    }

    clear_usage_clock_for_test();
    let _ = fs::remove_dir_all(&dir);
}

/// REQ-004/AC-004: a summary-write fault that fires *after* an earlier
/// contribution of the same batch has already been applied still rolls the whole
/// unit back. A real SQLite trigger aborts the target provider's summary insert
/// mid-batch, leaving the earlier contribution, every detail row and a prior
/// request's values unchanged; clearing the fault lets the same batch contribute
/// exactly once.
#[test]
fn append_batch_rolls_back_after_prior_summary_contribution() {
    let _seam = lock_seam();
    set_usage_clock_for_test(super::rfc3339_millis("2026-09-16T10:00:00+08:00"));
    let (dir, db_path) = temp_db("provider-summary-abort-prior");
    let store = UsageLogStore::at(&db_path);
    let now = super::rfc3339_millis("2026-09-16T12:00:00+08:00");
    let day = super::rfc3339_millis("2026-09-16T11:00:00+08:00");

    // A prior successful request establishes observable summary values.
    store
        .append_batch_with_accounting(&provider_batch("prior-p", day, 2), 365)
        .expect("the prior request must commit");
    let prior_before = store
        .provider_usage_history("prior-p", now)
        .expect("prior history before the fault");
    assert_eq!(
        prior_before
            .cumulative
            .as_ref()
            .map(|metrics| metrics.request_count),
        Some(1)
    );

    // The trigger aborts only the target contributor's summary insert, so the
    // other contributor has already been applied inside the batch transaction.
    let trigger = Connection::open(&db_path).expect("open raw db to arm the trigger");
    trigger
        .execute_batch(
            "CREATE TRIGGER abort_target_summary
             AFTER INSERT ON provider_daily_usage
             WHEN NEW.provider_id = 'target-p'
             BEGIN SELECT RAISE(ABORT, 'injected summary abort'); END;",
        )
        .expect("arm the summary abort trigger");
    drop(trigger);

    let failing: Vec<UsageLogEntry> = provider_batch("first-p", day, 1)
        .into_iter()
        .chain(provider_batch("target-p", day, 1))
        .collect();
    let error = store
        .append_batch_with_accounting(&failing, 365)
        .expect_err("the summary abort must fail the batch");
    assert!(!error.is_empty(), "the failure must carry a message");

    // No detail row and no summary contribution of the failed batch remains,
    // including the earlier contributor applied before the abort.
    assert_eq!(
        store.count().expect("detail rows after the fault"),
        2,
        "only the prior request's rows may remain"
    );
    assert!(
        store
            .all_records()
            .expect("records after the fault")
            .iter()
            .all(|row| row.provider_id != "first-p" && row.provider_id != "target-p"),
        "no row of the failed batch may remain"
    );
    for provider_id in ["first-p", "target-p"] {
        assert!(
            store
                .provider_usage_history(provider_id, now)
                .expect("provider history after the fault")
                .days
                .is_empty(),
            "{provider_id} must contribute no daily summary"
        );
    }
    let prior_after = store
        .provider_usage_history("prior-p", now)
        .expect("prior history after the fault");
    assert_eq!(
        prior_after.cumulative, prior_before.cumulative,
        "a rolled-back batch must not perturb prior summary values"
    );

    // Clearing the fault lets the same batch contribute exactly once.
    let clear = Connection::open(&db_path).expect("open raw db to drop the trigger");
    clear
        .execute_batch("DROP TRIGGER abort_target_summary")
        .expect("drop the summary abort trigger");
    drop(clear);
    store
        .append_batch_with_accounting(&failing, 365)
        .expect("the retried batch must commit");
    assert_eq!(store.count().expect("detail rows after retry"), 4);
    for provider_id in ["first-p", "target-p"] {
        let history = store
            .provider_usage_history(provider_id, now)
            .expect("provider history after retry");
        assert_eq!(history.days.len(), 1, "{provider_id} must contribute one day");
        assert_eq!(
            history.days[0]
                .metrics
                .as_ref()
                .map(|metrics| metrics.request_count),
            Some(1),
            "{provider_id} must contribute exactly one terminal request"
        );
    }

    clear_usage_clock_for_test();
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Step 2 (20261015-pool-only-reasoning) / AC-011: additive nullable pool
// evidence columns are added idempotently and never rewrite history.
// ---------------------------------------------------------------------------

/// AC-011: a pre-change `usage_logs` database opens twice without duplicating or
/// rewriting history; the historical row keeps its original tokens, amount and
/// effort, and reads back with present-null pool evidence; the two nullable
/// evidence columns exist after the additive migration.
#[test]
fn additive_pool_evidence_columns_open_twice_without_rewriting_history() {
    let (dir, db_path) = temp_db("usage-pool-evidence-additive");

    // A pre-change database: the historical column set carries no pool evidence.
    {
        let connection = rusqlite::Connection::open(&db_path).expect("open legacy db");
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
                    cost_breakdown TEXT,
                    first_token_ms REAL,
                    avg_tps REAL
                );
                INSERT INTO usage_logs (
                    timestamp_ms, local_model, upstream_model, provider_id, provider_name,
                    result, status, input_tokens, cache_read_tokens, cache_write_tokens,
                    output_tokens, total_tokens, amount, duration_ms, error_message, terminal,
                    reasoning_effort, cost_breakdown, first_token_ms, avg_tps
                ) VALUES (1000, 'legacy-model', 'legacy-up', 'p-legacy', 'Legacy', 'success', 200,
                    10, 2, 0, 5, 17, 0.25, 7, NULL, 1, 'high', NULL, NULL, NULL);",
            )
            .expect("create and seed the pre-change database");
    }

    let store = UsageLogStore::at(&db_path);
    let first_open = store.all_records().expect("first open reads history");
    let second_open = store.all_records().expect("second open reads history");
    assert_eq!(first_open.len(), 1, "the legacy row must survive");
    assert_eq!(second_open.len(), 1, "a reopen must not duplicate history");

    let record = &first_open[0];
    assert_eq!(record.local_model, "legacy-model");
    assert_eq!(
        record.reasoning_effort.as_deref(),
        Some("high"),
        "the historical effort must stay intact: {record:?}"
    );
    assert_eq!(record.input_tokens, 10, "historical tokens must not be rewritten");
    assert_eq!(record.total_tokens, 17);
    assert_eq!(record.amount, Some(0.25), "the historical amount must not be rewritten");

    let json = serde_json::to_value(record).expect("record serializes");
    assert_eq!(
        json.get("resolved_reasoning_effort"),
        Some(&Value::Null),
        "a historical row must carry a present-null resolved effort: {json}"
    );
    assert_eq!(
        json.get("native_reasoning"),
        Some(&Value::Null),
        "a historical row must carry a null native snapshot: {json}"
    );

    let connection = rusqlite::Connection::open(&db_path).expect("open for schema check");
    let mut statement = connection
        .prepare("PRAGMA table_info(usage_logs)")
        .expect("pragma table_info");
    let columns: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(1))
        .expect("query table_info")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect table_info");
    assert!(
        columns.iter().any(|column| column == "resolved_reasoning_effort"),
        "the additive resolved-effort column must exist: {columns:?}"
    );
    assert!(
        columns.iter().any(|column| column == "native_reasoning"),
        "the additive native-snapshot column must exist: {columns:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}
