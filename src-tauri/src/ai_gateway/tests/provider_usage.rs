//! REQ-001..REQ-006 (plan Step 1) behavior specification for retention-safe
//! provider usage history.
//!
//! This file is the single storage-boundary end-to-end specification for the
//! feature: it drives a real temporary SQLite database through
//! [`UsageLogStore`]'s public append paths, the provider identity/history
//! queries and the exact provider/day request-log query, then expires details,
//! reopens the store and asserts the durable observable results. All expected
//! values are written out independently of the implementation.
//!
//! Fixtures pin the planned internal injected clock so the migration
//! continuity boundary, the retention cutoff and the reporting day are
//! deterministic. Every query passes the same instant explicitly, so the
//! assertions hold whether the backend reads the injected clock or the
//! explicit argument.
//!
//! The assertions below are the intended public contract for the backend step;
//! they deliberately do not compile until that contract exists (truthful
//! initial RED for a brand-new feature).

use super::{make_temp_dir, rfc3339_millis, sample_attempt_record, sample_record, tokens};
use crate::ai_gateway::{
    clear_usage_clock_for_test, resolve_explicit_day, resolve_range, set_usage_clock_for_test,
    LogFilter, ProviderDailyEntry, ProviderDayCoverage, ProviderUsageHistory,
    ProviderUsageIdentity, TimeRange, UsageAccounting, UsageLogEntry, UsageLogStore, UsageResult,
    UsageTokens, USAGE_LOG_PAGE_SIZE,
};
use std::fs;
use std::path::PathBuf;

const DAY_MS: i64 = 86_400_000;

/// A temporary store whose internal clock is pinned, so the version-2
/// continuity boundary and every retention cutoff are deterministic. Dropping
/// it clears the clock seam and removes the temp directory.
struct PinnedStore {
    dir: PathBuf,
    store: UsageLogStore,
}

impl Drop for PinnedStore {
    fn drop(&mut self) {
        clear_usage_clock_for_test();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Pin the clock, create an empty database and force its first open (and
/// therefore its version-2 migration) so the continuity boundary is stamped
/// from the pinned instant.
fn pinned_store(name: &str, now: &str) -> PinnedStore {
    set_usage_clock_for_test(rfc3339_millis(now));
    let dir = make_temp_dir(name);
    fs::create_dir_all(&dir).expect("create temp dir");
    let store = UsageLogStore::at(dir.join("ai_gateway_usage.db"));
    let history = store
        .provider_usage_history("warmup", rfc3339_millis(now))
        .expect("initialize the usage database");
    assert!(
        history.days.is_empty(),
        "a brand-new database must have no history"
    );
    PinnedStore { dir, store }
}

/// Move the pinned clock without reopening the database (writes already hold
/// their connection and must not re-run the migration).
fn set_now(now: &str) {
    set_usage_clock_for_test(rfc3339_millis(now));
}

fn terminal(
    timestamp_ms: i64,
    local_model: &str,
    upstream_model: &str,
    provider_id: &str,
    provider_name: &str,
    result: UsageResult,
    amount: Option<f64>,
    counts: UsageTokens,
) -> UsageLogEntry {
    UsageLogEntry {
        record: sample_record(
            timestamp_ms,
            local_model,
            upstream_model,
            provider_id,
            provider_name,
            result,
            amount,
            counts,
        ),
        accounting: UsageAccounting::CANONICAL,
    }
}

/// A superseded attempt row of a request: non-terminal, so it is detail only.
fn superseded_attempt(
    timestamp_ms: i64,
    local_model: &str,
    provider_id: &str,
    provider_name: &str,
    counts: UsageTokens,
) -> UsageLogEntry {
    UsageLogEntry {
        record: sample_attempt_record(
            timestamp_ms,
            local_model,
            "remote-a",
            provider_id,
            provider_name,
            UsageResult::Success,
            false,
            None,
            Some(9.9),
            counts,
        ),
        accounting: UsageAccounting::CANONICAL,
    }
}

/// A terminal row whose upstream usage was absent/invalid: it still counts as a
/// request, but never enters the cache numerator/denominator.
fn ineligible_terminal(
    timestamp_ms: i64,
    local_model: &str,
    upstream_model: &str,
    provider_id: &str,
    provider_name: &str,
    result: UsageResult,
    amount: Option<f64>,
    counts: UsageTokens,
) -> UsageLogEntry {
    UsageLogEntry {
        record: sample_record(
            timestamp_ms,
            local_model,
            upstream_model,
            provider_id,
            provider_name,
            result,
            amount,
            counts,
        ),
        accounting: UsageAccounting::default(),
    }
}

fn append(store: &UsageLogStore, entries: &[UsageLogEntry], retention_days: u32) {
    store
        .append_batch_with_accounting(entries, retention_days)
        .expect("append usage entries");
}

fn identity<'a>(rows: &'a [ProviderUsageIdentity], id: &str) -> &'a ProviderUsageIdentity {
    rows.iter()
        .find(|row| row.provider_id == id)
        .unwrap_or_else(|| panic!("identity {id} missing from {rows:?}"))
}

fn day<'a>(history: &'a ProviderUsageHistory, day: &str) -> &'a ProviderDailyEntry {
    history
        .days
        .iter()
        .find(|entry| entry.day == day)
        .unwrap_or_else(|| panic!("day {day} missing from {:?}", history.days))
}

/// The calendar must be strictly ordered, gapless and end on the reporting day.
fn assert_calendar(history: &ProviderUsageHistory, first: &str, last: &str) {
    assert_eq!(history.earliest_available_day.as_deref(), Some(first));
    assert_eq!(history.end_day.as_deref(), Some(last));
    assert_eq!(
        history.days.first().map(|entry| entry.day.as_str()),
        Some(first)
    );
    assert_eq!(
        history.days.last().map(|entry| entry.day.as_str()),
        Some(last)
    );
    assert!(
        history
            .days
            .windows(2)
            .all(|pair| pair[0].day.as_str() < pair[1].day.as_str()),
        "calendar days must be strictly ascending: {:?}",
        history.days
    );
    assert!(
        history.days.iter().all(|entry| entry.day.len() == 10),
        "every day must be a full YYYY-MM-DD date: {:?}",
        history.days
    );
}

fn metrics(entry: &ProviderDailyEntry) -> &crate::ai_gateway::UsageMetrics {
    entry
        .metrics
        .as_ref()
        .unwrap_or_else(|| panic!("day {} must carry metrics", entry.day))
}

// ---------------------------------------------------------------------------
// REQ-001 / AC-001: stable identity and lifecycle
// ---------------------------------------------------------------------------

/// AC-001: the selector is the union of configured and historical identities
/// keyed by exact nonblank provider ID. The current configured name wins,
/// same-name IDs stay distinct, a deleted identity keeps its latest recorded
/// name and stays selectable with usage, and a configured-but-unused identity
/// has an empty history with no inferred first use.
#[test]
fn provider_usage_identity_list_unions_configured_and_historical() {
    let fixture = pinned_store("provider-identity", "2026-09-01T10:00:00+08:00");
    let store = &fixture.store;
    let when = rfc3339_millis("2026-09-01T11:00:00+08:00");
    append(
        store,
        &[
            terminal(
                when,
                "local-a",
                "remote-a",
                "alpha",
                "Alpha Old",
                UsageResult::Success,
                Some(1.0),
                tokens(1, 0, 0, 1),
            ),
            terminal(
                when,
                "local-a",
                "remote-a",
                "beta",
                "Alpha",
                UsageResult::Success,
                Some(1.0),
                tokens(1, 0, 0, 1),
            ),
            terminal(
                when,
                "local-a",
                "remote-a",
                "gamma",
                "Deleted Provider",
                UsageResult::Success,
                Some(1.0),
                tokens(1, 0, 0, 1),
            ),
        ],
        365,
    );

    // "alpha" was renamed in configuration; "gamma" was deleted; "delta" is a
    // configured provider that has never received traffic.
    let configured = vec![
        ("alpha".to_string(), "Alpha Renamed".to_string()),
        ("beta".to_string(), "Alpha".to_string()),
        ("delta".to_string(), "Delta".to_string()),
    ];
    let identities = store
        .provider_usage_identities(&configured)
        .expect("provider identities");
    assert_eq!(
        identities.len(),
        4,
        "the union of configured and historical identities: {identities:?}"
    );

    let alpha = identity(&identities, "alpha");
    assert_eq!(
        alpha.provider_name, "Alpha Renamed",
        "the current configured name wins over the recorded one"
    );
    assert!(!alpha.deleted);
    assert!(alpha.has_usage);

    let beta = identity(&identities, "beta");
    assert_eq!(beta.provider_name, "Alpha");
    assert!(!beta.deleted, "a configured identity is not deleted");
    assert!(beta.has_usage);

    let gamma = identity(&identities, "gamma");
    assert_eq!(
        gamma.provider_name, "Deleted Provider",
        "a deleted identity keeps its latest recorded name"
    );
    assert!(gamma.deleted, "a historical identity absent from config is deleted");
    assert!(gamma.has_usage, "a deleted identity with history stays selectable");

    let delta = identity(&identities, "delta");
    assert_eq!(delta.provider_name, "Delta");
    assert!(!delta.deleted);
    assert!(!delta.has_usage, "a configured provider without traffic has no usage");

    // A renamed ID keeps one continuous series.
    let alpha_history = store
        .provider_usage_history("alpha", when)
        .expect("renamed provider history");
    assert_calendar(&alpha_history, "2026-09-01", "2026-09-01");
    assert_eq!(metrics(day(&alpha_history, "2026-09-01")).request_count, 1);

    // An unused configured provider has no fabricated timeline.
    let delta_history = store
        .provider_usage_history("delta", when)
        .expect("unused provider history");
    assert!(delta_history.days.is_empty(), "unused provider has no calendar");
    assert!(delta_history.earliest_available_day.is_none());
    assert!(delta_history.cumulative.is_none(), "no inferred cumulative data");

    // An unknown nonblank ID has empty history, never an error.
    let unknown = store
        .provider_usage_history("unknown-id", when)
        .expect("unknown ids are empty, not errors");
    assert!(unknown.days.is_empty());
    assert!(unknown.earliest_available_day.is_none());
}

// ---------------------------------------------------------------------------
// REQ-002 / AC-002 / AC-005: daily calendar and coverage
// ---------------------------------------------------------------------------

/// AC-002 / AC-005: every UTC+8 date from the earliest available record through
/// today appears once. Retained days before the saved continuity boundary keep
/// their metrics; a pre-boundary date without retained data is unavailable
/// (never a numeric zero); a date inside continuous coverage with no requests
/// is a genuine zero; today is identified as ongoing.
#[test]
fn provider_history_calendar_classifies_retained_unavailable_and_covered_zero() {
    let fixture = pinned_store("provider-calendar", "2026-09-05T10:00:00+08:00");
    let store = &fixture.store;

    // Two pre-boundary retained days with a missing day between them.
    append(
        store,
        &[
            terminal(
                rfc3339_millis("2026-08-30T12:00:00+08:00"),
                "local-a",
                "remote-a",
                "alpha",
                "Alpha",
                UsageResult::Success,
                Some(0.3),
                tokens(1, 0, 0, 0),
            ),
            terminal(
                rfc3339_millis("2026-09-01T12:00:00+08:00"),
                "local-a",
                "remote-a",
                "alpha",
                "Alpha",
                UsageResult::Success,
                Some(0.4),
                tokens(2, 0, 0, 0),
            ),
        ],
        365,
    );

    let now = rfc3339_millis("2026-09-05T12:00:00+08:00");
    let history = store
        .provider_usage_history("alpha", now)
        .expect("provider history");
    assert_calendar(&history, "2026-08-30", "2026-09-05");
    assert_eq!(
        history.continuity_start_day.as_deref(),
        Some("2026-09-05"),
        "the migration day is the continuous-recording boundary"
    );

    let retained = day(&history, "2026-08-30");
    assert_eq!(retained.coverage, ProviderDayCoverage::RetainedHistorical);
    assert_eq!(
        retained.metrics.as_ref().map(|m| m.request_count),
        Some(1),
        "a retained historical day keeps its recorded metrics"
    );

    let missing = day(&history, "2026-08-31");
    assert_eq!(
        missing.coverage,
        ProviderDayCoverage::Unavailable,
        "a missing pre-boundary date is unavailable"
    );
    assert!(
        missing.metrics.is_none(),
        "an unavailable historical date must not be a numeric zero"
    );

    let covered = day(&history, "2026-09-05");
    assert_eq!(covered.coverage, ProviderDayCoverage::Continuous);
    assert!(covered.ongoing, "the current UTC+8 day is ongoing");
    assert_eq!(
        metrics(covered).request_count,
        0,
        "a covered no-request date is a genuine zero"
    );
    assert_eq!(
        metrics(covered).cache_hit_rate_percent, None,
        "no eligible denominator means an unavailable rate, not 0%"
    );

    for missing_day in ["2026-09-02", "2026-09-03", "2026-09-04"] {
        let entry = day(&history, missing_day);
        assert_eq!(entry.coverage, ProviderDayCoverage::Unavailable);
        assert!(entry.metrics.is_none());
    }
    let first_retained = day(&history, "2026-09-01");
    assert_eq!(
        first_retained.coverage,
        ProviderDayCoverage::RetainedHistorical
    );
    assert_eq!(metrics(first_retained).request_count, 1);

    // Cumulative sums only the available days.
    let cumulative = history.cumulative.expect("cumulative for a used provider");
    assert_eq!(
        cumulative.request_count, 2,
        "cumulative excludes unavailable dates"
    );
    assert!((cumulative.amount - 0.7).abs() < 1e-9);
}

/// AC-002: first use today yields exactly one ongoing daily bucket, and UTC+8
/// midnight — not a fixed UTC midnight — separates neighboring days.
#[test]
fn provider_history_first_use_today_is_one_ongoing_bucket() {
    let fixture = pinned_store("provider-first-today", "2026-09-05T10:00:00+08:00");
    let store = &fixture.store;
    append(
        store,
        &[terminal(
            rfc3339_millis("2026-09-05T12:00:00+08:00"),
            "local-a",
            "remote-a",
            "alpha",
            "Alpha",
            UsageResult::Success,
            Some(0.1),
            tokens(1, 0, 0, 1),
        )],
        365,
    );
    let now = rfc3339_millis("2026-09-05T13:00:00+08:00");
    let history = store
        .provider_usage_history("alpha", now)
        .expect("provider history");
    assert_calendar(&history, "2026-09-05", "2026-09-05");
    assert_eq!(history.days.len(), 1, "first use today is one bucket");
    assert!(history.days[0].ongoing);
    assert_eq!(history.days[0].coverage, ProviderDayCoverage::Continuous);
    assert_eq!(metrics(&history.days[0]).request_count, 1);

    // A 23:59:59 record and a 00:00:00 record are adjacent UTC+8 days.
    let midnight = pinned_store("provider-midnight", "2026-09-05T10:00:00+08:00");
    append(
        &midnight.store,
        &[
            terminal(
                rfc3339_millis("2026-09-05T23:59:59+08:00"),
                "local-a",
                "remote-a",
                "alpha",
                "Alpha",
                UsageResult::Success,
                Some(0.1),
                tokens(1, 0, 0, 0),
            ),
            terminal(
                rfc3339_millis("2026-09-06T00:00:00+08:00"),
                "local-a",
                "remote-a",
                "alpha",
                "Alpha",
                UsageResult::Success,
                Some(0.2),
                tokens(2, 0, 0, 0),
            ),
        ],
        365,
    );
    let boundary = midnight
        .store
        .provider_usage_history("alpha", rfc3339_millis("2026-09-06T01:00:00+08:00"))
        .expect("provider history");
    assert_calendar(&boundary, "2026-09-05", "2026-09-06");
    assert_eq!(
        metrics(day(&boundary, "2026-09-05")).request_count,
        1,
        "23:59:59 belongs to its own UTC+8 day"
    );
    assert_eq!(metrics(day(&boundary, "2026-09-06")).request_count, 1);
    assert!(day(&boundary, "2026-09-06").ongoing);
}

/// AC-002: full dates are retained across a multi-year history; older daily
/// summaries survive detail expiry instead of being silently discarded.
#[test]
fn provider_history_multi_year_keeps_every_date() {
    let fixture = pinned_store("provider-multi-year", "2024-01-05T10:00:00+08:00");
    let store = &fixture.store;
    append(
        store,
        &[terminal(
            rfc3339_millis("2024-01-05T12:00:00+08:00"),
            "local-a",
            "remote-a",
            "alpha",
            "Alpha",
            UsageResult::Success,
            Some(0.1),
            tokens(1, 0, 0, 1),
        )],
        365,
    );

    // Move two years forward and write a fresh request: retention expires the
    // 2024 detail row but must keep its daily summary.
    set_now("2026-09-20T10:00:00+08:00");
    append(
        store,
        &[terminal(
            rfc3339_millis("2026-09-20T12:00:00+08:00"),
            "local-a",
            "remote-a",
            "alpha",
            "Alpha",
            UsageResult::Success,
            Some(0.2),
            tokens(2, 0, 0, 0),
        )],
        365,
    );

    let history = store
        .provider_usage_history("alpha", rfc3339_millis("2026-09-20T12:30:00+08:00"))
        .expect("multi-year provider history");
    assert_calendar(&history, "2024-01-05", "2026-09-20");

    let expected_days = (chrono::NaiveDate::from_ymd_opt(2026, 9, 20).expect("date")
        - chrono::NaiveDate::from_ymd_opt(2024, 1, 5).expect("date"))
    .num_days() as usize
        + 1;
    assert_eq!(
        history.days.len(),
        expected_days,
        "a multi-year fixture must retain every date"
    );
    assert_eq!(
        metrics(day(&history, "2024-01-05")).request_count,
        1,
        "the 2024 daily summary survives detail expiry"
    );
    assert_eq!(metrics(day(&history, "2026-09-20")).request_count, 1);
    assert!(
        day(&history, "2026-01-01").metrics.is_some(),
        "continuous days keep numeric metrics"
    );

    // The 2024 detail row itself is gone.
    assert!(
        store
            .all_records()
            .expect("detail rows")
            .iter()
            .all(|row| row.timestamp_ms != rfc3339_millis("2024-01-05T12:00:00+08:00")),
        "retention must delete the expired detail row"
    );
}

// ---------------------------------------------------------------------------
// REQ-003 / AC-003: metric reconciliation
// ---------------------------------------------------------------------------

/// AC-003: daily and cumulative results reconcile to retained terminal rows
/// before cleanup and keep the same persisted values after cleanup. Only the
/// terminal record counts; superseded attempts and empty-provider failures do
/// not accrue; sums are additive; the cache rate is the summed eligible
/// numerator over the summed eligible denominator; a genuine 0% stays distinct
/// from an unavailable rate; stored amounts are never recomputed.
#[test]
fn provider_history_metrics_reconcile_and_survive_detail_cleanup() {
    let fixture = pinned_store("provider-metrics", "2026-09-10T10:00:00+08:00");
    let store = &fixture.store;
    let ts = rfc3339_millis("2026-09-10T11:00:00+08:00");
    // A retained day before the continuity boundary carries a nonzero
    // cache-write tier and a legacy stored total that differs from its
    // four-tier sum, so the additive stored value — never a recomputed sum — is
    // what the daily and cumulative metrics must expose.
    let prior_ts = rfc3339_millis("2026-09-09T11:00:00+08:00");
    let mut prior_priced = sample_record(
        prior_ts,
        "local-a",
        "remote-a",
        "alpha",
        "Alpha",
        UsageResult::Success,
        Some(0.2),
        tokens(8, 0, 4, 2),
    );
    prior_priced.total_tokens = 7;
    append(
        store,
        &[
            UsageLogEntry {
                record: prior_priced,
                accounting: UsageAccounting::CANONICAL,
            },
            ineligible_terminal(
                prior_ts,
                "local-a",
                "remote-a",
                "alpha",
                "Alpha",
                UsageResult::Success,
                Some(0.1),
                tokens(3, 0, 0, 1),
            ),
        ],
        365,
    );
    append(
        store,
        &[
            // r1: priced success with a cache read.
            terminal(ts, "local-a", "remote-a", "alpha", "Alpha", UsageResult::Success, Some(1.0), tokens(10, 2, 0, 3)),
            // r2: unpriced success.
            terminal(ts, "local-a", "remote-x", "alpha", "Alpha", UsageResult::Success, None, tokens(1, 0, 0, 1)),
            // r3: explicit zero price stays priced.
            terminal(ts, "local-a", "remote-a", "alpha", "Alpha", UsageResult::Success, Some(0.0), tokens(2, 0, 0, 2)),
            // r4: partial-usage failure counts and is unpriced.
            terminal(ts, "local-a", "remote-y", "alpha", "Alpha", UsageResult::Failure, None, tokens(5, 0, 0, 0)),
            // r5: failure with no readable upstream is not unpriced.
            terminal(ts, "local-a", "", "alpha", "Alpha", UsageResult::Failure, None, tokens(0, 0, 0, 0)),
            // r6: absent/invalid usage still counts as a request, never cached.
            ineligible_terminal(ts, "local-a", "remote-a", "alpha", "Alpha", UsageResult::Success, Some(0.1), tokens(3, 0, 0, 3)),
            // r7: superseded attempt, detail only.
            superseded_attempt(ts, "local-a", "alpha", "Alpha", tokens(50, 50, 50, 50)),
            // r8: synthetic empty-provider failure never accrues.
            terminal(ts, "local-a", "remote-a", "", "", UsageResult::Failure, None, tokens(0, 0, 0, 0)),
        ],
        365,
    );

    let now = rfc3339_millis("2026-09-10T12:00:00+08:00");
    let history = store.provider_usage_history("alpha", now).expect("history");
    assert_calendar(&history, "2026-09-09", "2026-09-10");
    let daily_prior = *metrics(day(&history, "2026-09-09"));
    assert_eq!(daily_prior.request_count, 2);
    assert_eq!(daily_prior.successful_request_count, 2);
    assert_eq!(daily_prior.input_tokens, 11);
    assert_eq!(
        daily_prior.cache_write_tokens, 4,
        "a nonzero cache-write tier is summed, not dropped"
    );
    assert_eq!(
        daily_prior.total_tokens, 11,
        "the legacy stored total is authoritative instead of the four-tier sum (18)"
    );
    assert_eq!(daily_prior.cache_hit_tokens, 0);
    assert_eq!(daily_prior.cache_eligible_tokens, 12);
    assert_eq!(daily_prior.cache_rate_eligible_count, 1);
    assert_eq!(
        daily_prior.cache_hit_rate_percent,
        Some(0.0),
        "an eligible day with no cache hits is a genuine 0%, never unavailable"
    );
    let daily = *metrics(day(&history, "2026-09-10"));
    assert_eq!(daily.request_count, 6, "only terminal nonblank-provider rows count");
    assert_eq!(daily.successful_request_count, 4);
    assert_eq!(
        daily.request_count - daily.successful_request_count,
        2,
        "failure count is request minus successful"
    );
    assert_eq!(daily.input_tokens, 21);
    assert_eq!(daily.cache_read_tokens, 2);
    assert_eq!(daily.cache_write_tokens, 0);
    assert_eq!(daily.output_tokens, 9);
    assert_eq!(daily.total_tokens, 32);
    assert!((daily.amount - 1.1).abs() < 1e-9, "stored amounts are summed as recorded");
    assert_eq!(daily.unpriced_count, 2);
    assert_eq!(daily.cache_hit_tokens, 2);
    assert_eq!(daily.cache_eligible_tokens, 15);
    assert_eq!(daily.cache_rate_eligible_count, 3);
    let rate = daily.cache_hit_rate_percent.expect("eligible rate");
    assert!((rate - (2.0 / 15.0 * 100.0)).abs() < 1e-9);

    let cumulative = history.cumulative.expect("cumulative");
    assert_eq!(cumulative.request_count, 8);
    assert_eq!(cumulative.successful_request_count, 6);
    assert_eq!(cumulative.input_tokens, 32);
    assert_eq!(cumulative.cache_read_tokens, 2);
    assert_eq!(cumulative.cache_write_tokens, 4);
    assert_eq!(cumulative.output_tokens, 12);
    assert_eq!(cumulative.total_tokens, 43);
    assert!((cumulative.amount - 1.4).abs() < 1e-9);
    assert_eq!(cumulative.unpriced_count, 2);
    assert_eq!(cumulative.cache_hit_tokens, 2);
    assert_eq!(cumulative.cache_eligible_tokens, 27);
    assert_eq!(cumulative.cache_rate_eligible_count, 4);
    let weighted = cumulative.cache_hit_rate_percent.expect("summed eligible rate");
    assert!(
        (weighted - (2.0 / 27.0 * 100.0)).abs() < 1e-9,
        "the rate must be the summed numerator over the summed denominator"
    );
    let averaged_percentages = ((2.0 / 15.0 * 100.0) + (0.0 / 12.0 * 100.0)) / 2.0;
    assert!(
        (weighted - averaged_percentages).abs() > 0.5,
        "the summed rate must not be an average of the two daily percentages"
    );

    // Expire the detail rows with a one-day retention and reopen. The fresh
    // row carries absent/invalid usage, so it must not perturb the cache sums.
    set_now("2026-09-12T10:00:00+08:00");
    append(
        store,
        &[ineligible_terminal(
            rfc3339_millis("2026-09-12T11:00:00+08:00"),
            "local-a",
            "remote-a",
            "alpha",
            "Alpha",
            UsageResult::Success,
            Some(0.5),
            tokens(1, 0, 0, 0),
        )],
        1,
    );
    let reopened = UsageLogStore::at(fixture.dir.join("ai_gateway_usage.db"));
    let after = reopened
        .provider_usage_history("alpha", rfc3339_millis("2026-09-12T12:00:00+08:00"))
        .expect("history after cleanup");
    assert_calendar(&after, "2026-09-09", "2026-09-12");
    assert_eq!(
        *metrics(day(&after, "2026-09-10")),
        daily,
        "the persisted daily summary survives detail cleanup"
    );
    assert_eq!(
        *metrics(day(&after, "2026-09-09")),
        daily_prior,
        "the pre-boundary retained summary survives detail cleanup"
    );
    assert_eq!(metrics(day(&after, "2026-09-11")).request_count, 0);
    assert_eq!(
        metrics(day(&after, "2026-09-11")).cache_hit_rate_percent,
        None,
        "a covered no-request day keeps an unavailable rate, distinct from a genuine 0%"
    );
    assert_eq!(
        metrics(day(&after, "2026-09-09")).cache_hit_rate_percent,
        Some(0.0),
        "the genuine 0% survives cleanup"
    );
    assert_eq!(metrics(day(&after, "2026-09-12")).request_count, 1);
    let after_cumulative = after.cumulative.expect("cumulative after cleanup");
    assert_eq!(after_cumulative.request_count, 9);
    assert_eq!(
        after_cumulative.cache_eligible_tokens, 27,
        "cache denominators are summed across days, never averaged"
    );
    assert!(
        reopened
            .all_records()
            .expect("detail rows after cleanup")
            .iter()
            .all(|row| row.timestamp_ms != ts && row.timestamp_ms != prior_ts),
        "the expired detail rows must be gone"
    );
}

// ---------------------------------------------------------------------------
// REQ-005 / AC-006: exact provider/day request details
// ---------------------------------------------------------------------------

/// AC-006: an explicit day is a strict UTC+8 half-open window that takes
/// precedence over the quick range, so the following midnight is excluded. The
/// provider-ID query keeps attempt-row visibility, the fixed 50-row pagination
/// and the ordinary status/model/name filters and facets; unknown nonblank IDs
/// return empty data.
#[test]
fn provider_day_logs_use_exact_utc8_window_and_compose_filters() {
    let fixture = pinned_store("provider-day-logs", "2026-09-11T10:00:00+08:00");
    let store = &fixture.store;
    let day_start = rfc3339_millis("2026-09-11T00:00:00+08:00");

    let mut entries = Vec::new();
    for index in 0..55i64 {
        let result = if index % 5 == 0 {
            UsageResult::Failure
        } else {
            UsageResult::Success
        };
        let model = if index % 2 == 0 { "m1" } else { "m2" };
        entries.push(terminal(
            day_start + index * 1_000,
            model,
            "remote-a",
            "alpha",
            "Alpha",
            result,
            Some(0.1),
            tokens(1, 0, 0, 1),
        ));
    }
    // One superseded attempt on the same day stays visible in the details.
    entries.push(superseded_attempt(
        day_start + 1_000_000,
        "m1",
        "alpha",
        "Alpha",
        tokens(7, 0, 0, 0),
    ));
    // The first instant of the next day is excluded from this window.
    entries.push(terminal(
        rfc3339_millis("2026-09-12T00:00:00+08:00"),
        "m1",
        "remote-a",
        "alpha",
        "Alpha",
        UsageResult::Success,
        Some(0.1),
        tokens(1, 0, 0, 1),
    ));
    // A different provider on the same day never leaks into alpha's details.
    entries.push(terminal(
        day_start + 2_000,
        "m1",
        "remote-b",
        "beta",
        "Beta",
        UsageResult::Success,
        Some(0.1),
        tokens(1, 0, 0, 1),
    ));
    append(store, &entries, 365);

    let now = rfc3339_millis("2026-09-12T10:00:00+08:00");
    let quick = resolve_range(Some(7), now);
    let exact = resolve_explicit_day(Some("2026-09-11"), quick).expect("valid day");
    assert_eq!(exact.start_ms, Some(day_start));
    assert_eq!(exact.end_ms, Some(day_start + DAY_MS));

    let page_one = store
        .query_provider_logs(&exact, &LogFilter::default(), "alpha", 1)
        .expect("day page one");
    assert_eq!(USAGE_LOG_PAGE_SIZE, 50);
    assert_eq!(page_one.page, 1);
    assert_eq!(page_one.page_size, 50);
    assert_eq!(
        page_one.total, 56,
        "55 terminal + 1 superseded attempt, and neither the next midnight nor beta"
    );
    assert_eq!(page_one.total_pages, 2);
    assert_eq!(page_one.records.len(), 50);
    assert!(
        page_one.records.iter().all(|row| row.provider_id == "alpha"),
        "details are the exact provider ID"
    );
    assert!(
        page_one.records.iter().all(|row| row.timestamp_ms >= day_start
            && row.timestamp_ms < day_start + DAY_MS),
        "details stay inside the half-open UTC+8 day"
    );
    assert!(
        page_one.records.iter().any(|row| !row.terminal),
        "the superseded attempt row must remain visible"
    );

    let page_two = store
        .query_provider_logs(&exact, &LogFilter::default(), "alpha", 2)
        .expect("day page two");
    assert_eq!(page_two.page, 2);
    assert_eq!(page_two.records.len(), 6);

    // Ordinary filters compose with the exact ID/day boundary.
    let failures = store
        .query_provider_logs(
            &exact,
            &LogFilter {
                status: Some(UsageResult::Failure),
                model: None,
                provider: None,
            },
            "alpha",
            1,
        )
        .expect("day failures");
    assert_eq!(failures.total, 11, "indices 0,5,...,50");
    assert!(failures
        .records
        .iter()
        .all(|row| row.result == UsageResult::Failure && row.provider_id == "alpha"));

    let m1 = store
        .query_provider_logs(
            &exact,
            &LogFilter {
                status: None,
                model: Some("m1".to_string()),
                provider: None,
            },
            "alpha",
            1,
        )
        .expect("day m1");
    assert!(m1.records.iter().all(|row| row.local_model == "m1"));
    assert!(
        m1.models.contains(&"m1".to_string()) && m1.models.contains(&"m2".to_string()),
        "the model facet still spans the whole day"
    );

    // Unknown nonblank IDs are empty, not errors.
    let unknown = store
        .query_provider_logs(&exact, &LogFilter::default(), "ghost", 1)
        .expect("unknown provider day");
    assert_eq!(unknown.total, 0);
    assert!(unknown.records.is_empty());
}

/// AC-006: invalid calendar dates and explicitly blank IDs return actionable
/// errors rather than unfiltered data; omitting the day keeps the quick range.
#[test]
fn provider_day_window_rejects_invalid_dates_and_blank_ids() {
    let fixture = pinned_store("provider-day-errors", "2026-09-11T10:00:00+08:00");
    let store = &fixture.store;
    let now = rfc3339_millis("2026-09-11T10:00:00+08:00");
    let quick = resolve_range(Some(7), now);

    assert_eq!(
        resolve_explicit_day(None, quick).expect("no day keeps the quick range"),
        quick
    );
    let blank_day = resolve_explicit_day(Some("   "), quick).expect_err("a blank day must fail");
    assert!(!blank_day.is_empty(), "the error must carry a message");
    assert!(resolve_explicit_day(Some("2026-02-30"), quick).is_err());
    assert!(resolve_explicit_day(Some("2026-2-3"), quick).is_err());

    // A valid leap day and a year boundary resolve to strict UTC+8 windows.
    let leap_start = rfc3339_millis("2024-02-29T00:00:00+08:00");
    let leap = resolve_explicit_day(Some("2024-02-29"), quick).expect("leap day");
    assert_eq!(leap.start_ms, Some(leap_start));
    assert_eq!(leap.end_ms, Some(leap_start + DAY_MS));
    assert!(resolve_explicit_day(Some("2026-02-29"), quick).is_err());
    let year_start = rfc3339_millis("2026-12-31T00:00:00+08:00");
    let year_end = resolve_explicit_day(Some("2026-12-31"), quick).expect("year boundary");
    assert_eq!(year_end.start_ms, Some(year_start));
    assert_eq!(year_end.end_ms, Some(rfc3339_millis("2027-01-01T00:00:00+08:00")));

    // A blank explicit ID is rejected, not treated as "no filter".
    let day = resolve_explicit_day(Some("2026-09-11"), quick).expect("valid day");
    let error = store
        .query_provider_logs(&day, &LogFilter::default(), "  ", 1)
        .expect_err("a blank provider id must be rejected");
    assert!(!error.is_empty(), "the blank-id error must carry a message");
    let by_name_range = TimeRange::default();
    let _ = store
        .query_provider_logs(&by_name_range, &LogFilter::default(), "alpha", 1)
        .expect("a real provider id still works");
}

// ---------------------------------------------------------------------------
// REQ-001 / AC-001: exact nonblank identity, equal names and latest recorded name
// ---------------------------------------------------------------------------

/// AC-001: the selector is keyed by exact, nonblank provider ID. Explicitly
/// blank and whitespace-only configured IDs are never selectable, while a
/// whitespace-suffixed nonblank ID is preserved verbatim and stays distinct from
/// its trimmed form.
#[test]
fn provider_usage_identities_skip_blank_ids_and_keep_exact_strings() {
    let fixture = pinned_store("provider-identity-exact", "2026-09-01T10:00:00+08:00");
    let store = &fixture.store;
    let when = rfc3339_millis("2026-09-01T11:00:00+08:00");
    append(
        store,
        &[terminal(
            when,
            "local-a",
            "remote-a",
            "alpha",
            "Alpha",
            UsageResult::Success,
            Some(1.0),
            tokens(1, 0, 0, 1),
        )],
        365,
    );

    let configured = vec![
        ("alpha".to_string(), "Alpha".to_string()),
        ("".to_string(), "Blank".to_string()),
        ("   ".to_string(), "Whitespace".to_string()),
        ("alpha ".to_string(), "Trailing Space".to_string()),
    ];
    let identities = store
        .provider_usage_identities(&configured)
        .expect("provider identities");
    assert!(
        identities
            .iter()
            .all(|row| !row.provider_id.trim().is_empty()),
        "blank and whitespace-only configured ids must never become selectable: {identities:?}"
    );
    assert_eq!(
        identities.len(),
        2,
        "only the two nonblank configured ids remain: {identities:?}"
    );

    let trimmed = identity(&identities, "alpha");
    assert_eq!(trimmed.provider_name, "Alpha");
    assert!(trimmed.has_usage, "the recorded id stays selectable");
    let suffixed = identity(&identities, "alpha ");
    assert_eq!(suffixed.provider_name, "Trailing Space");
    assert!(
        !suffixed.has_usage,
        "the whitespace-suffixed id is distinct from its trimmed form"
    );
}

/// AC-001: two different IDs sharing one display name stay two distinct series,
/// and one ID with several recorded names remains a single series whose deleted
/// identity takes the latest recorded name even when the rows arrive
/// out of order.
#[test]
fn provider_usage_identities_keep_equal_names_distinct_and_latest_recorded_name() {
    let fixture = pinned_store("provider-identity-names", "2026-09-01T10:00:00+08:00");
    let store = &fixture.store;
    let early = rfc3339_millis("2026-09-01T11:00:00+08:00");
    let late = rfc3339_millis("2026-09-01T12:00:00+08:00");
    // Newest-first append order: the latest recorded name must still win.
    append(
        store,
        &[
            terminal(
                late,
                "local-a",
                "remote-a",
                "delta",
                "Delta Newest",
                UsageResult::Success,
                Some(1.0),
                tokens(1, 0, 0, 1),
            ),
            terminal(
                early,
                "local-a",
                "remote-a",
                "delta",
                "Delta Older",
                UsageResult::Success,
                Some(1.0),
                tokens(1, 0, 0, 1),
            ),
        ],
        365,
    );

    let configured = vec![
        ("beta".to_string(), "Shared".to_string()),
        ("gamma".to_string(), "Shared".to_string()),
    ];
    let identities = store
        .provider_usage_identities(&configured)
        .expect("provider identities");
    let beta = identity(&identities, "beta");
    let gamma = identity(&identities, "gamma");
    assert_eq!(beta.provider_name, "Shared");
    assert_eq!(gamma.provider_name, "Shared");
    assert_ne!(
        beta.provider_id, gamma.provider_id,
        "an equal display name must stay two distinct series"
    );
    assert!(!beta.has_usage);
    assert!(!gamma.has_usage);

    let delta = identity(&identities, "delta");
    assert!(delta.deleted, "an id absent from the configuration is deleted");
    assert_eq!(
        delta.provider_name, "Delta Newest",
        "the latest recorded name wins under out-of-order input"
    );

    // The renamed records remain exactly one continuous series.
    let history = store.provider_usage_history("delta", late).expect("delta history");
    assert_calendar(&history, "2026-09-01", "2026-09-01");
    assert_eq!(
        metrics(day(&history, "2026-09-01")).request_count,
        2,
        "renamed records stay one series"
    );
}
