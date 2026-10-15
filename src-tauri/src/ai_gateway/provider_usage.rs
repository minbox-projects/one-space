//! Durable provider/day usage summaries and exact provider/day history
//! (plan Step 1, REQ-001..REQ-005).
//!
//! `provider_daily_usage` stores only additive metrics for provider/day keys
//! that actually recorded terminal use, plus the recorded identity and first/
//! last request timestamps; `provider_usage_metadata` stores coverage facts such
//! as the continuous-recording boundary. Calendar dates, the zero-versus-
//! unavailable classification and cumulative sums are generated on read, so no
//! empty day is ever stored and detail retention can never lose a summary.
//!
//! Table creation, the one-shot backfill and the continuity stamp run inside the
//! permanent migration's serialized write transaction; the append paths fold
//! each terminal row into the same transaction as its detail row.

use super::usage_log::{
    now_millis, TimeRange, UsageLogEntry, UsageLogStore, UsageMetrics, UsageResult,
    ELIGIBLE_ROW_SQL, UNPRICED_ROW_SQL,
};
use chrono::{NaiveDate, TimeZone};
use rusqlite::{Connection, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Milliseconds in one UTC+8 natural day.
const DAY_MS: i64 = 86_400_000;

/// Additive provider/day summary and coverage tables. Keyed by exact provider ID
/// plus the UTC+8 `YYYY-MM-DD` day; the metadata table records migration facts.
const SUMMARY_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS provider_daily_usage (
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
CREATE TABLE IF NOT EXISTS provider_usage_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
";

/// Metadata key holding the UTC+8 day from which continuous provider summaries
/// are guaranteed; earlier retained days may be incomplete.
const CONTINUITY_START_DAY_KEY: &str = "continuity_start_day";

/// Additive summary upsert shared by the single and batch append paths. The
/// newest recorded name wins by request timestamp so an out-of-order append
/// cannot regress it.
const SUMMARY_UPSERT_SQL: &str = "
INSERT INTO provider_daily_usage (
    provider_id, day, provider_name,
    request_count, successful_request_count,
    input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, total_tokens,
    amount, unpriced_count, cache_hit_tokens, cache_eligible_tokens,
    cache_rate_eligible_count, first_request_at_ms, last_request_at_ms
) VALUES (?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(provider_id, day) DO UPDATE SET
    provider_name = CASE
        WHEN excluded.last_request_at_ms >= provider_daily_usage.last_request_at_ms
        THEN excluded.provider_name
        ELSE provider_daily_usage.provider_name
    END,
    request_count = provider_daily_usage.request_count + 1,
    successful_request_count = provider_daily_usage.successful_request_count
        + excluded.successful_request_count,
    input_tokens = provider_daily_usage.input_tokens + excluded.input_tokens,
    cache_read_tokens = provider_daily_usage.cache_read_tokens + excluded.cache_read_tokens,
    cache_write_tokens = provider_daily_usage.cache_write_tokens + excluded.cache_write_tokens,
    output_tokens = provider_daily_usage.output_tokens + excluded.output_tokens,
    total_tokens = provider_daily_usage.total_tokens + excluded.total_tokens,
    amount = provider_daily_usage.amount + excluded.amount,
    unpriced_count = provider_daily_usage.unpriced_count + excluded.unpriced_count,
    cache_hit_tokens = provider_daily_usage.cache_hit_tokens + excluded.cache_hit_tokens,
    cache_eligible_tokens = provider_daily_usage.cache_eligible_tokens
        + excluded.cache_eligible_tokens,
    cache_rate_eligible_count = provider_daily_usage.cache_rate_eligible_count
        + excluded.cache_rate_eligible_count,
    first_request_at_ms = MIN(provider_daily_usage.first_request_at_ms, excluded.first_request_at_ms),
    last_request_at_ms = MAX(provider_daily_usage.last_request_at_ms, excluded.last_request_at_ms)
";

/// One selectable provider identity, keyed by exact nonblank provider ID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderUsageIdentity {
    pub provider_id: String,
    pub provider_name: String,
    pub deleted: bool,
    pub has_usage: bool,
}

/// Coverage classification of one UTC+8 calendar day in a provider history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderDayCoverage {
    /// Retained data recorded before continuous coverage began; may be
    /// incomplete and only describes available records.
    RetainedHistorical,
    /// Inside known continuous coverage: a no-request day is a genuine zero.
    Continuous,
    /// Before continuous coverage with no retained record: metrics unavailable.
    Unavailable,
}

/// One calendar day of a provider history; `metrics` is `None` only when the
/// date is unavailable (never a fabricated zero).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderDailyEntry {
    pub day: String,
    pub coverage: ProviderDayCoverage,
    pub ongoing: bool,
    pub metrics: Option<UsageMetrics>,
}

/// Complete provider history: the earliest retained day, the continuity
/// boundary, the current reporting day and the generated calendar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderUsageHistory {
    pub provider_id: String,
    pub earliest_available_day: Option<String>,
    pub continuity_start_day: Option<String>,
    pub end_day: Option<String>,
    pub cumulative: Option<UsageMetrics>,
    pub days: Vec<ProviderDailyEntry>,
}

// ---------------------------------------------------------------------------
// UTC+8 day arithmetic
// ---------------------------------------------------------------------------

fn utc8_offset() -> chrono::FixedOffset {
    chrono::FixedOffset::east_opt(8 * 3600).expect("UTC+8 is a valid fixed offset")
}

/// UTC+8 `YYYY-MM-DD` label of an epoch-millisecond instant.
fn utc8_day_label(millis: i64) -> String {
    chrono::DateTime::from_timestamp_millis(millis)
        .map(|instant| {
            instant
                .with_timezone(&utc8_offset())
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_default()
}

/// Strict `YYYY-MM-DD` parse: a lenient parse such as `2026-2-3` is rejected
/// because the round-trip label differs from the input.
fn parse_utc8_day(value: &str) -> Result<NaiveDate, String> {
    let invalid = || format!("invalid day '{value}': expected a YYYY-MM-DD UTC+8 date");
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| invalid())?;
    if date.format("%Y-%m-%d").to_string() != value {
        return Err(invalid());
    }
    Ok(date)
}

fn utc8_day_start_ms(date: NaiveDate) -> i64 {
    let midnight = date.and_hms_opt(0, 0, 0).expect("midnight exists");
    utc8_offset()
        .from_local_datetime(&midnight)
        .single()
        .expect("UTC+8 midnight is unambiguous")
        .timestamp_millis()
}

/// Resolve an optional strict `YYYY-MM-DD` UTC+8 day to the half-open window
/// `[midnight, next midnight)` that overrides any quick range. `None` keeps
/// `quick_range`; an explicitly blank or invalid day is an actionable error.
pub(in crate::ai_gateway) fn resolve_explicit_day(
    day: Option<&str>,
    quick_range: TimeRange,
) -> Result<TimeRange, String> {
    let Some(raw) = day else {
        return Ok(quick_range);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("a day must be a nonblank YYYY-MM-DD date".to_string());
    }
    let date = parse_utc8_day(trimmed)?;
    let start = utc8_day_start_ms(date);
    Ok(TimeRange {
        start_ms: Some(start),
        end_ms: Some(start + DAY_MS),
    })
}

// ---------------------------------------------------------------------------
// Migration hooks (called from the permanent migration inside its transaction)
// ---------------------------------------------------------------------------

fn has_table(connection: &Connection, name: &str) -> Result<bool, String> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
            [name],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    Ok(count > 0)
}

/// Create the additive summary/coverage tables inside the caller's transaction,
/// so a failed migration leaves no partial table behind.
pub(in crate::ai_gateway) fn create_summary_tables(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(SUMMARY_SCHEMA)
        .map_err(|error| error.to_string())
}

/// One accumulated provider/day summary during backfill.
struct DaySummary {
    provider_name: String,
    request_count: i64,
    successful_request_count: i64,
    input_tokens: i64,
    cache_read_tokens: i64,
    cache_write_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
    amount: f64,
    unpriced_count: i64,
    cache_hit_tokens: i64,
    cache_eligible_tokens: i64,
    cache_rate_eligible_count: i64,
    first_request_at_ms: i64,
    last_request_at_ms: i64,
}

/// Backfill retained terminal, valid-result (`success`/`failure`), nonblank-
/// provider rows exactly once into `provider_daily_usage`. Ambiguous attempts,
/// deleted `cancelled` rows and synthetic empty-provider failures never
/// contribute; original detail columns, tokens and amounts are never rewritten.
pub(in crate::ai_gateway) fn backfill_provider_summaries(
    connection: &Connection,
) -> Result<(), String> {
    // Reuse the shared eligibility/unpriced SQL predicates so the backfill can
    // never drift from the live metric projection. Ascending (timestamp, id)
    // order makes the "latest recorded name" deterministic for tied timestamps.
    let mut statement = connection
        .prepare(&format!(
            "SELECT provider_id, timestamp_ms, provider_name, result, \
                    input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, \
                    total_tokens, amount, \
                    ({ELIGIBLE_ROW_SQL}) AS eligible, ({UNPRICED_ROW_SQL}) AS unpriced \
             FROM usage_logs \
             WHERE terminal = 1 AND provider_id <> '' AND result IN ('success', 'failure') \
             ORDER BY timestamp_ms ASC, id ASC"
        ))
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, Option<f64>>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, i64>(11)?,
            ))
        })
        .map_err(|error| error.to_string())?;

    let mut summaries: HashMap<(String, String), DaySummary> = HashMap::new();
    for row in rows {
        let (
            provider_id,
            timestamp_ms,
            provider_name,
            result,
            input_tokens,
            cache_read_tokens,
            cache_write_tokens,
            output_tokens,
            total_tokens,
            amount,
            eligible,
            unpriced,
        ) = row.map_err(|error| error.to_string())?;
        // A whitespace-only provider id is not a selectable identity: skip it
        // exactly like the empty id the query excludes, matching the nonblank
        // rule the append path and the configured selector already use.
        if provider_id.trim().is_empty() {
            continue;
        }
        let day = utc8_day_label(timestamp_ms);
        let success = result == "success";
        let eligible = eligible != 0;
        let unpriced = unpriced != 0;
        let summary = summaries
            .entry((provider_id, day))
            .or_insert_with(|| DaySummary {
                provider_name: provider_name.clone(),
                request_count: 0,
                successful_request_count: 0,
                input_tokens: 0,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                amount: 0.0,
                unpriced_count: 0,
                cache_hit_tokens: 0,
                cache_eligible_tokens: 0,
                cache_rate_eligible_count: 0,
                first_request_at_ms: timestamp_ms,
                last_request_at_ms: timestamp_ms,
            });
        if timestamp_ms >= summary.last_request_at_ms {
            summary.provider_name = provider_name;
            summary.last_request_at_ms = timestamp_ms;
        }
        if timestamp_ms <= summary.first_request_at_ms {
            summary.first_request_at_ms = timestamp_ms;
        }
        summary.request_count += 1;
        if success {
            summary.successful_request_count += 1;
        }
        summary.input_tokens += input_tokens;
        summary.cache_read_tokens += cache_read_tokens;
        summary.cache_write_tokens += cache_write_tokens;
        summary.output_tokens += output_tokens;
        summary.total_tokens += total_tokens;
        summary.amount += amount.unwrap_or(0.0);
        if unpriced {
            summary.unpriced_count += 1;
        }
        if eligible {
            summary.cache_hit_tokens += cache_read_tokens;
            summary.cache_eligible_tokens +=
                input_tokens + cache_read_tokens + cache_write_tokens;
            summary.cache_rate_eligible_count += 1;
        }
    }
    drop(statement);

    let mut insert = connection
        .prepare(
            "INSERT INTO provider_daily_usage (
                provider_id, day, provider_name, request_count, successful_request_count,
                input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, total_tokens,
                amount, unpriced_count, cache_hit_tokens, cache_eligible_tokens,
                cache_rate_eligible_count, first_request_at_ms, last_request_at_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .map_err(|error| error.to_string())?;
    for ((provider_id, day), summary) in summaries {
        insert
            .execute(rusqlite::params![
                provider_id,
                day,
                summary.provider_name,
                summary.request_count,
                summary.successful_request_count,
                summary.input_tokens,
                summary.cache_read_tokens,
                summary.cache_write_tokens,
                summary.output_tokens,
                summary.total_tokens,
                summary.amount,
                summary.unpriced_count,
                summary.cache_hit_tokens,
                summary.cache_eligible_tokens,
                summary.cache_rate_eligible_count,
                summary.first_request_at_ms,
                summary.last_request_at_ms,
            ])
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Stamp the migration UTC+8 day as the continuous-coverage boundary. The
/// smallest detail retention is one rolling day, so today's earlier records are
/// still present when this runs.
pub(in crate::ai_gateway) fn stamp_continuity_boundary(
    connection: &Connection,
) -> Result<(), String> {
    let day = utc8_day_label(now_millis());
    connection
        .execute(
            "INSERT OR IGNORE INTO provider_usage_metadata (key, value) VALUES (?, ?)",
            rusqlite::params![CONTINUITY_START_DAY_KEY, day],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Append-time summary contribution
// ---------------------------------------------------------------------------

#[cfg(test)]
thread_local! {
    /// One-shot summary-update fault armed on the current thread, so arming it
    /// in one test can never be consumed by a concurrent append in another.
    static PROVIDER_USAGE_SUMMARY_FAILURE: std::cell::Cell<bool> = std::cell::Cell::new(false);
}

/// Arm a one-shot provider/day summary-update fault on the current thread (test
/// seam).
#[cfg(test)]
pub(in crate::ai_gateway) fn inject_provider_usage_summary_failure() {
    PROVIDER_USAGE_SUMMARY_FAILURE.with(|cell| cell.set(true));
}

/// Disarm any pending summary-update fault on the current thread (test seam).
#[cfg(test)]
pub(in crate::ai_gateway) fn clear_provider_usage_summary_failure() {
    PROVIDER_USAGE_SUMMARY_FAILURE.with(|cell| cell.set(false));
}

fn provider_usage_summary_fault_should_fail() -> Result<(), String> {
    #[cfg(test)]
    {
        let armed = PROVIDER_USAGE_SUMMARY_FAILURE.with(|cell| cell.replace(false));
        if armed {
            return Err("injected provider usage summary failure".to_string());
        }
    }
    Ok(())
}

/// Upsert each terminal nonblank-provider entry's additive contribution inside
/// the append transaction, so detail insertion and summary contribution commit
/// or roll back together. Attempts and empty-provider rows never contribute.
pub(in crate::ai_gateway) fn apply_summary_contributions(
    transaction: &Transaction<'_>,
    entries: &[UsageLogEntry],
) -> Result<(), String> {
    let contributors: Vec<&UsageLogEntry> = entries
        .iter()
        .filter(|entry| entry.record.terminal && !entry.record.provider_id.trim().is_empty())
        .collect();
    if contributors.is_empty() {
        return Ok(());
    }
    provider_usage_summary_fault_should_fail()?;
    let mut statement = transaction
        .prepare(SUMMARY_UPSERT_SQL)
        .map_err(|error| error.to_string())?;
    for entry in contributors {
        let record = &entry.record;
        let day = utc8_day_label(record.timestamp_ms);
        let success = record.result == UsageResult::Success;
        let eligible = success
            && entry.accounting.present
            && entry.accounting.valid
            && record.input_tokens + record.cache_read_tokens + record.cache_write_tokens > 0;
        let unpriced = record.amount.is_none()
            && !record.upstream_model.is_empty()
            && (success || record.total_tokens > 0);
        let cache_denominator =
            record.input_tokens + record.cache_read_tokens + record.cache_write_tokens;
        statement
            .execute(rusqlite::params![
                record.provider_id,
                day,
                record.provider_name,
                success as i64,
                record.input_tokens as i64,
                record.cache_read_tokens as i64,
                record.cache_write_tokens as i64,
                record.output_tokens as i64,
                record.total_tokens as i64,
                record.amount.unwrap_or(0.0),
                unpriced as i64,
                if eligible {
                    record.cache_read_tokens as i64
                } else {
                    0
                },
                if eligible { cache_denominator as i64 } else { 0 },
                eligible as i64,
                record.timestamp_ms,
                record.timestamp_ms,
            ])
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Store queries
// ---------------------------------------------------------------------------

fn metrics_from_summary_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(String, UsageMetrics)> {
    let day: String = row.get(0)?;
    let cache_hit_tokens = row.get::<_, i64>(10)? as u64;
    let cache_eligible_tokens = row.get::<_, i64>(11)? as u64;
    let cache_hit_rate_percent = if cache_eligible_tokens > 0 {
        Some(cache_hit_tokens as f64 * 100.0 / cache_eligible_tokens as f64)
    } else {
        None
    };
    Ok((
        day,
        UsageMetrics {
            request_count: row.get::<_, i64>(1)? as u32,
            input_tokens: row.get::<_, i64>(3)? as u64,
            cache_read_tokens: row.get::<_, i64>(4)? as u64,
            cache_write_tokens: row.get::<_, i64>(5)? as u64,
            output_tokens: row.get::<_, i64>(6)? as u64,
            total_tokens: row.get::<_, i64>(7)? as u64,
            amount: row.get::<_, f64>(8)?,
            unpriced_count: row.get::<_, i64>(9)? as u32,
            cache_hit_tokens,
            cache_eligible_tokens,
            cache_hit_rate_percent,
            cache_rate_eligible_count: row.get::<_, i64>(12)? as u32,
            successful_request_count: row.get::<_, i64>(2)? as u32,
        },
    ))
}

fn accumulate_metrics(total: &mut UsageMetrics, add: &UsageMetrics) {
    total.request_count += add.request_count;
    total.successful_request_count += add.successful_request_count;
    total.input_tokens += add.input_tokens;
    total.cache_read_tokens += add.cache_read_tokens;
    total.cache_write_tokens += add.cache_write_tokens;
    total.output_tokens += add.output_tokens;
    total.total_tokens += add.total_tokens;
    total.amount += add.amount;
    total.unpriced_count += add.unpriced_count;
    total.cache_hit_tokens += add.cache_hit_tokens;
    total.cache_eligible_tokens += add.cache_eligible_tokens;
    total.cache_rate_eligible_count += add.cache_rate_eligible_count;
}

fn derive_rate(metrics: &mut UsageMetrics) {
    metrics.cache_hit_rate_percent = if metrics.cache_eligible_tokens > 0 {
        Some(metrics.cache_hit_tokens as f64 * 100.0 / metrics.cache_eligible_tokens as f64)
    } else {
        None
    };
}

impl UsageLogStore {
    /// The union of configured and historical provider identities keyed by exact
    /// nonblank provider ID. The current configured name wins; a historical id
    /// absent from the configuration is `deleted` and keeps its latest recorded
    /// name; an unused configured provider has `has_usage = false`.
    pub(in crate::ai_gateway) fn provider_usage_identities(
        &self,
        configured: &[(String, String)],
    ) -> Result<Vec<ProviderUsageIdentity>, String> {
        let connection = self.open()?;
        let mut historical: HashMap<String, String> = HashMap::new();
        if has_table(&connection, "provider_daily_usage")? {
            let mut statement = connection
                .prepare(
                    "SELECT provider_id, provider_name FROM provider_daily_usage \
                     ORDER BY last_request_at_ms ASC, day ASC",
                )
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|error| error.to_string())?;
            for row in rows {
                let (provider_id, provider_name) = row.map_err(|error| error.to_string())?;
                // A blank or whitespace-only id is never a selectable identity.
                if provider_id.trim().is_empty() {
                    continue;
                }
                // Ascending order leaves the latest recorded name per id, with
                // the day as a deterministic tie-break.
                historical.insert(provider_id, provider_name);
            }
        }
        let mut identities = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for (provider_id, provider_name) in configured {
            if provider_id.trim().is_empty() {
                continue;
            }
            if !seen.insert(provider_id.clone()) {
                continue;
            }
            identities.push(ProviderUsageIdentity {
                provider_id: provider_id.clone(),
                provider_name: provider_name.clone(),
                deleted: false,
                has_usage: historical.contains_key(provider_id),
            });
        }
        let mut remaining: Vec<(String, String)> = historical
            .into_iter()
            .filter(|(provider_id, _)| !seen.contains(provider_id))
            .collect();
        remaining.sort_by(|left, right| left.0.cmp(&right.0));
        for (provider_id, provider_name) in remaining {
            identities.push(ProviderUsageIdentity {
                provider_id,
                provider_name,
                deleted: true,
                has_usage: true,
            });
        }
        Ok(identities)
    }

    /// One consistent read of the provider history: the full UTC+8 calendar from
    /// the earliest available day through the reporting day, per-day coverage,
    /// and cumulative sums over available days only. A blank id is an actionable
    /// error; a nonblank id is matched verbatim so adjacent identities never
    /// collapse into one series.
    pub(in crate::ai_gateway) fn provider_usage_history(
        &self,
        provider_id: &str,
        now_ms: i64,
    ) -> Result<ProviderUsageHistory, String> {
        if provider_id.trim().is_empty() {
            return Err("a provider id must be nonblank".to_string());
        }
        let connection = self.open()?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|error| error.to_string())?;

        let continuity_start_day = if has_table(&transaction, "provider_usage_metadata")? {
            transaction
                .query_row(
                    "SELECT value FROM provider_usage_metadata WHERE key = ?",
                    [CONTINUITY_START_DAY_KEY],
                    |row| row.get::<_, String>(0),
                )
                .ok()
        } else {
            None
        };

        let mut summaries: HashMap<String, UsageMetrics> = HashMap::new();
        if has_table(&transaction, "provider_daily_usage")? {
            let mut statement = transaction
                .prepare(
                    "SELECT day, request_count, successful_request_count, input_tokens, \
                            cache_read_tokens, cache_write_tokens, output_tokens, total_tokens, \
                            amount, unpriced_count, cache_hit_tokens, cache_eligible_tokens, \
                            cache_rate_eligible_count \
                     FROM provider_daily_usage WHERE provider_id = ? ORDER BY day ASC",
                )
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map([provider_id], metrics_from_summary_row)
                .map_err(|error| error.to_string())?;
            for row in rows {
                let (day, metrics) = row.map_err(|error| error.to_string())?;
                summaries.insert(day, metrics);
            }
        }
        drop(transaction);

        let mut available_days: Vec<String> = summaries.keys().cloned().collect();
        available_days.sort();
        let earliest_available_day = available_days.first().cloned();

        let Some(earliest_label) = earliest_available_day.clone() else {
            return Ok(ProviderUsageHistory {
                provider_id: provider_id.to_string(),
                earliest_available_day: None,
                continuity_start_day,
                end_day: None,
                cumulative: None,
                days: Vec::new(),
            });
        };

        let mut cursor = parse_utc8_day(&earliest_label)?;
        let end_label = utc8_day_label(now_ms);
        let end = parse_utc8_day(&end_label)?;

        let mut days = Vec::new();
        let mut cumulative = UsageMetrics::default();
        let mut has_available_metrics = false;
        while cursor <= end {
            let label = cursor.format("%Y-%m-%d").to_string();
            let covered = continuity_start_day
                .as_deref()
                .is_some_and(|start| label.as_str() >= start);
            let (coverage, metrics) = match summaries.get(&label) {
                Some(metrics) => (
                    if covered {
                        ProviderDayCoverage::Continuous
                    } else {
                        ProviderDayCoverage::RetainedHistorical
                    },
                    Some(*metrics),
                ),
                None if covered => (
                    ProviderDayCoverage::Continuous,
                    Some(UsageMetrics::default()),
                ),
                None => (ProviderDayCoverage::Unavailable, None),
            };
            if let Some(metrics) = metrics.as_ref() {
                accumulate_metrics(&mut cumulative, metrics);
                has_available_metrics = true;
            }
            days.push(ProviderDailyEntry {
                day: label.clone(),
                coverage,
                ongoing: label == end_label && coverage == ProviderDayCoverage::Continuous,
                metrics,
            });
            cursor = cursor
                .succ_opt()
                .ok_or_else(|| "provider history day overflow".to_string())?;
        }

        if has_available_metrics {
            derive_rate(&mut cumulative);
        }
        Ok(ProviderUsageHistory {
            provider_id: provider_id.to_string(),
            earliest_available_day,
            continuity_start_day,
            end_day: Some(end_label),
            cumulative: has_available_metrics.then_some(cumulative),
            days,
        })
    }
}
