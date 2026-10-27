//! SQL statistics, record projection and query methods for the usage log.

use super::range::{day_index_label, TimeRange};
use super::schema::RECORD_COLUMNS;
use super::records::{
    LogFilter, UsageBucketRow, UsageLogGroupRow, UsageLogRecord, UsageLogsPage, UsageMetrics,
    UsageModelRow, UsageProviderModelRow, UsageProviderRow, UsageProviderStatsRow, UsageResult,
    UsageStats, UnpricedUsageItem,
};
use super::store::UsageLogStore;
use rusqlite::{params_from_iter, Row};
use std::collections::HashMap;

/// Fixed page size for the ungrouped request-log list.
pub const USAGE_LOG_PAGE_SIZE: u32 = 50;

/// SQL predicate selecting a cache-eligible row: an in-range
/// successful terminal row with a present, valid canonical decomposition and a
/// positive cache denominator. Totals, buckets, models, providers and the
/// one-shot migration backfill all reuse this one predicate so no level can
/// drift from another.
pub(in crate::ai_gateway) const ELIGIBLE_ROW_SQL: &str = "result = 'success' \
    AND usage_semantics = 'canonical_v1' \
    AND usage_present = 1 \
    AND cache_accounting_valid = 1 \
    AND (input_tokens + cache_read_tokens + cache_write_tokens) > 0";

/// SQL predicate selecting an unpriced billable row: a missing amount with a
/// readable upstream model that is either successful or carries recorded total
/// tokens. Free models are excluded because they are expected to have no
/// configured price: an upstream id ending in `-free` or `:free`, or a local
/// model using the `zen-free-<slug>` naming convention. Names that merely
/// contain `free` and paid upstreams behind a free-sounding local alias stay
/// unpriced. The metric projection and provider daily history share it.
pub(in crate::ai_gateway) const UNPRICED_ROW_SQL: &str = "amount IS NULL \
    AND upstream_model <> '' \
    AND (result = 'success' OR total_tokens > 0) \
    AND lower(trim(upstream_model)) NOT LIKE '%-free' \
    AND lower(trim(upstream_model)) NOT LIKE '%:free' \
    AND lower(trim(local_model)) NOT LIKE 'zen-free-%'";

/// The shared metric projection, in [`metrics_from_row`] order. The final four
/// aggregates are the additive cache fields; the percentage is derived from the
/// two sums in Rust so every level uses the same token weighting.
fn metric_columns() -> String {
    format!(
        "COUNT(*), \
         COALESCE(SUM(input_tokens), 0), \
         COALESCE(SUM(cache_read_tokens), 0), \
         COALESCE(SUM(cache_write_tokens), 0), \
         COALESCE(SUM(output_tokens), 0), \
         COALESCE(SUM(total_tokens), 0), \
         COALESCE(SUM(amount), 0.0), \
         COALESCE(SUM(CASE WHEN {UNPRICED_ROW_SQL} THEN 1 ELSE 0 END), 0), \
         COALESCE(SUM(CASE WHEN result = 'success' THEN 1 ELSE 0 END), 0), \
         COALESCE(SUM(CASE WHEN {ELIGIBLE_ROW_SQL} THEN cache_read_tokens ELSE 0 END), 0), \
         COALESCE(SUM(CASE WHEN {ELIGIBLE_ROW_SQL} THEN input_tokens + cache_read_tokens + cache_write_tokens ELSE 0 END), 0), \
         COALESCE(SUM(CASE WHEN {ELIGIBLE_ROW_SQL} THEN 1 ELSE 0 END), 0)"
    )
}

fn metrics_from_row(row: &Row<'_>, offset: usize) -> rusqlite::Result<UsageMetrics> {
    let cache_hit_tokens = row.get::<_, i64>(offset + 9)? as u64;
    let cache_eligible_tokens = row.get::<_, i64>(offset + 10)? as u64;
    let cache_hit_rate_percent = if cache_eligible_tokens > 0 {
        Some(cache_hit_tokens as f64 * 100.0 / cache_eligible_tokens as f64)
    } else {
        None
    };
    Ok(UsageMetrics {
        request_count: row.get::<_, i64>(offset)? as u32,
        input_tokens: row.get::<_, i64>(offset + 1)? as u64,
        cache_read_tokens: row.get::<_, i64>(offset + 2)? as u64,
        cache_write_tokens: row.get::<_, i64>(offset + 3)? as u64,
        output_tokens: row.get::<_, i64>(offset + 4)? as u64,
        total_tokens: row.get::<_, i64>(offset + 5)? as u64,
        amount: row.get::<_, f64>(offset + 6)?,
        unpriced_count: row.get::<_, i64>(offset + 7)? as u32,
        successful_request_count: row.get::<_, i64>(offset + 8)? as u32,
        cache_hit_tokens,
        cache_eligible_tokens,
        cache_hit_rate_percent,
        cache_rate_eligible_count: row.get::<_, i64>(offset + 11)? as u32,
    })
}

/// Add one aggregation's additive metrics into another. The cache percentage is
/// derived from the summed components by [`derive_cache_rate`], never averaged.
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

fn derive_cache_rate(metrics: &mut UsageMetrics) {
    metrics.cache_hit_rate_percent = if metrics.cache_eligible_tokens > 0 {
        Some(metrics.cache_hit_tokens as f64 * 100.0 / metrics.cache_eligible_tokens as f64)
    } else {
        None
    };
}

/// Reproject the whole-attempt average TPS from the persisted output count and
/// total duration, without rewriting the database.
///
/// The stored `avg_tps` column is an availability marker only: a row measured by
/// an attempt observation (stored value present) is recomputed as
/// `output_tokens / (duration_ms / 1000)` from its own persisted values, so
/// every read path — `all_records`, filtered/grouped pages and reopened
/// databases — reports the whole-attempt rate (including the pre-output wait)
/// and a historical row measured under an obsolete divisor is corrected on read.
/// A stored `NULL` stays unknown; an explicit zero count over a positive
/// duration stays `0.0`; a non-positive duration yields `None` rather than an
/// infinite rate.
fn projected_avg_tps(
    stored_avg_tps: Option<f64>,
    output_tokens: u64,
    duration_ms: u64,
) -> Option<f64> {
    if stored_avg_tps.is_none() || duration_ms == 0 {
        return None;
    }
    Some(output_tokens as f64 / (duration_ms as f64 / 1000.0))
}

pub(super) fn record_from_row(row: &Row<'_>) -> rusqlite::Result<UsageLogRecord> {
    let result: String = row.get(5)?;
    // Reuse the persisted count and duration for both the record fields and the
    // whole-attempt TPS projection, so no value is fetched twice.
    let output_tokens = row.get::<_, i64>(10)? as u64;
    let duration_ms = row.get::<_, i64>(13)? as u64;
    let avg_tps = projected_avg_tps(row.get::<_, Option<f64>>(19)?, output_tokens, duration_ms);
    let cost_breakdown = row
        .get::<_, Option<String>>(17)?
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                17,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let native_reasoning = row
        .get::<_, Option<String>>(21)?
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                21,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    Ok(UsageLogRecord {
        timestamp_ms: row.get(0)?,
        local_model: row.get(1)?,
        upstream_model: row.get(2)?,
        provider_id: row.get(3)?,
        provider_name: row.get(4)?,
        result: UsageResult::parse(&result).unwrap_or(UsageResult::Failure),
        status: row.get::<_, i64>(6)? as u16,
        input_tokens: row.get::<_, i64>(7)? as u64,
        cache_read_tokens: row.get::<_, i64>(8)? as u64,
        cache_write_tokens: row.get::<_, i64>(9)? as u64,
        output_tokens,
        total_tokens: row.get::<_, i64>(11)? as u64,
        amount: row.get::<_, Option<f64>>(12)?,
        cost_breakdown,
        duration_ms,
        error_message: row.get::<_, Option<String>>(14)?,
        terminal: row.get::<_, i64>(15)? != 0,
        reasoning_effort: row.get::<_, Option<String>>(16)?,
        first_token_ms: row.get::<_, Option<f64>>(18)?,
        avg_tps,
        resolved_reasoning_effort: row.get::<_, Option<String>>(20)?,
        native_reasoning,
    })
}

/// Build the `WHERE` clause shared by every user-visible log query.
///
/// `terminal_only` adds the statistics/grouping filter; the ungrouped list keeps
/// every row that survives the filter. The optional `provider_id` boundary is
/// composed with AND like every other selector so an exact provider/day
/// drilldown reuses the same predicate for rows, groups and facets.
fn bind_boundary(
    range: &TimeRange,
    filter: &LogFilter,
    terminal_only: bool,
    provider_id: Option<&str>,
) -> (String, Vec<rusqlite::types::Value>) {
    let mut clauses: Vec<&str> = Vec::new();
    let mut params: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(start) = range.start_ms {
        clauses.push("timestamp_ms >= ?");
        params.push(start.into());
    }
    if let Some(end) = range.end_ms {
        clauses.push("timestamp_ms < ?");
        params.push(end.into());
    }
    if terminal_only {
        clauses.push("terminal = 1");
    }
    if let Some(status) = filter.status {
        clauses.push("result = ?");
        params.push(status.as_str().to_string().into());
    }
    if let Some(model) = filter.model.as_deref().filter(|model| !model.is_empty()) {
        clauses.push("local_model = ?");
        params.push(model.to_string().into());
    }
    if let Some(provider) = filter
        .provider
        .as_deref()
        .filter(|provider| !provider.trim().is_empty())
    {
        clauses.push("provider_name = ?");
        params.push(provider.to_string().into());
    }
    if let Some(provider_id) = provider_id {
        clauses.push("provider_id = ?");
        params.push(provider_id.to_string().into());
    }
    let where_sql = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    (where_sql, params)
}

/// [`bind_boundary`] without a provider-id boundary; the ordinary query surface.
fn bind(
    range: &TimeRange,
    filter: &LogFilter,
    terminal_only: bool,
) -> (String, Vec<rusqlite::types::Value>) {
    bind_boundary(range, filter, terminal_only, None)
}

impl UsageLogStore {
    /// Ungrouped page of records, newest first, 50 per page. `page` is clamped
    /// to the valid range so switching ranges never lands on a blank page.
    pub(in crate::ai_gateway) fn query_logs(
        &self,
        range: &TimeRange,
        filter: &LogFilter,
        page: u32,
    ) -> Result<UsageLogsPage, String> {
        self.query_logs_filtered(range, filter, None, page)
    }

    /// Exact provider/day page: the recorded provider-ID boundary composes with
    /// the ordinary name/status/model filters and feeds the same predicate used
    /// for the facets. A blank explicit id is an actionable error, never "no
    /// filter"; an unknown nonblank id yields an empty page. A nonblank id is
    /// matched verbatim so adjacent identities never collapse.
    pub(in crate::ai_gateway) fn query_provider_logs(
        &self,
        range: &TimeRange,
        filter: &LogFilter,
        provider_id: &str,
        page: u32,
    ) -> Result<UsageLogsPage, String> {
        if provider_id.trim().is_empty() {
            return Err("a provider id must be nonblank".to_string());
        }
        self.query_logs_filtered(range, filter, Some(provider_id), page)
    }

    fn query_logs_filtered(
        &self,
        range: &TimeRange,
        filter: &LogFilter,
        provider_id: Option<&str>,
        page: u32,
    ) -> Result<UsageLogsPage, String> {
        self.query_connection(|query| {
            let (where_sql, params) = bind_boundary(range, filter, false, provider_id);
            let total: i64 = query
                .prepare(&format!("SELECT COUNT(*) FROM usage_logs{where_sql}"))?
                .query_row(params_from_iter(params.iter()), |row| row.get(0))
                .map_err(|error| error.to_string())?;
            let total = total as u32;
            let total_pages = total.div_ceil(USAGE_LOG_PAGE_SIZE).max(1);
            let page = page.max(1).min(total_pages);
            let offset = (page - 1) * USAGE_LOG_PAGE_SIZE;
            let mut statement = query.prepare(&format!(
                "SELECT {RECORD_COLUMNS}
                 FROM usage_logs{where_sql}
                 ORDER BY timestamp_ms DESC, id DESC
                 LIMIT ? OFFSET ?"
            ))?;
            let mut page_params = params;
            page_params.push((USAGE_LOG_PAGE_SIZE as i64).into());
            page_params.push((offset as i64).into());
            let records = statement
                .query_map(params_from_iter(page_params.iter()), record_from_row)
                .map_err(|error| error.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|error| error.to_string())?;
            // The model facet spans the whole range and ignores the model filter,
            // so the filter selector can offer in-range models absent from this
            // page. It honors the active status and provider filters.
            let model_facet_filter = LogFilter {
                status: filter.status,
                model: None,
                provider: filter.provider.clone(),
            };
            let (model_facet_where_sql, model_facet_params) =
                bind_boundary(range, &model_facet_filter, false, provider_id);
            let model_facet_where = if model_facet_where_sql.is_empty() {
                " WHERE local_model <> ''".to_string()
            } else {
                format!("{model_facet_where_sql} AND local_model <> ''")
            };
            let mut model_facet_statement = query.prepare(&format!(
                "SELECT DISTINCT local_model FROM usage_logs{model_facet_where} ORDER BY local_model ASC"
            ))?;
            let models = model_facet_statement
                .query_map(params_from_iter(model_facet_params.iter()), |row| {
                    row.get::<_, String>(0)
                })
                .map_err(|error| error.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|error| error.to_string())?;
            // The provider facet spans the whole range, ignores the provider
            // filter itself so the selection can be changed without clearing it,
            // and honors the active status and model filters. Blank names (the
            // synthetic no-provider row) are excluded.
            let provider_facet_filter = LogFilter {
                status: filter.status,
                model: filter.model.clone(),
                provider: None,
            };
            let (provider_facet_where_sql, provider_facet_params) =
                bind_boundary(range, &provider_facet_filter, false, provider_id);
            let provider_facet_where = if provider_facet_where_sql.is_empty() {
                " WHERE provider_name <> ''".to_string()
            } else {
                format!("{provider_facet_where_sql} AND provider_name <> ''")
            };
            let mut provider_facet_statement = query.prepare(&format!(
                "SELECT DISTINCT provider_name FROM usage_logs{provider_facet_where} ORDER BY provider_name ASC"
            ))?;
            let providers = provider_facet_statement
                .query_map(params_from_iter(provider_facet_params.iter()), |row| {
                    row.get::<_, String>(0)
                })
                .map_err(|error| error.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|error| error.to_string())?;
            Ok(UsageLogsPage {
                page,
                page_size: USAGE_LOG_PAGE_SIZE,
                total,
                total_pages,
                group_by: None,
                records,
                groups: Vec::new(),
                models,
                providers,
            })
        })
    }

    /// Grouped rows by `"model"`, `"provider"` or `"day"` (UTC+8). `error_count`
    /// counts only `failure` rows.
    pub(in crate::ai_gateway) fn group_logs(
        &self,
        range: &TimeRange,
        filter: &LogFilter,
        group_by: &str,
    ) -> Result<Vec<UsageLogGroupRow>, String> {
        self.group_logs_filtered(range, filter, None, group_by)
    }

    /// Grouped rows restricted by an exact provider-ID boundary that composes
    /// with the ordinary group filters. A nonblank id is matched verbatim.
    pub(in crate::ai_gateway) fn group_logs_with_provider(
        &self,
        range: &TimeRange,
        filter: &LogFilter,
        provider_id: &str,
        group_by: &str,
    ) -> Result<Vec<UsageLogGroupRow>, String> {
        if provider_id.trim().is_empty() {
            return Err("a provider id must be nonblank".to_string());
        }
        self.group_logs_filtered(range, filter, Some(provider_id), group_by)
    }

    fn group_logs_filtered(
        &self,
        range: &TimeRange,
        filter: &LogFilter,
        provider_id: Option<&str>,
        group_by: &str,
    ) -> Result<Vec<UsageLogGroupRow>, String> {
        self.query_connection(|query| {
            let (where_sql, params) = bind_boundary(range, filter, true, provider_id);
            let mut groups = Vec::new();
            if group_by == "day" {
                let sql = format!(
                    "SELECT ((timestamp_ms + 28800000) / 86400000) AS day_index,
                            COUNT(*),
                            COALESCE(SUM(CASE WHEN result = 'failure' THEN 1 ELSE 0 END), 0),
                            COALESCE(MAX(timestamp_ms), 0)
                     FROM usage_logs{where_sql}
                     GROUP BY day_index ORDER BY day_index ASC"
                );
                let mut statement = query.prepare(&sql)?;
                let rows = statement
                    .query_map(params_from_iter(params.iter()), |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    })
                    .map_err(|error| error.to_string())?;
                for row in rows {
                    let (index, request_count, error_count, last_request_at_ms) =
                        row.map_err(|error| error.to_string())?;
                    groups.push(UsageLogGroupRow {
                        group: day_index_label(index),
                        request_count: request_count as u32,
                        error_count: error_count as u32,
                        last_request_at_ms,
                    });
                }
            } else if group_by == "provider" {
                let sql = format!(
                    "SELECT provider_name,
                            COUNT(*),
                            COALESCE(SUM(CASE WHEN result = 'failure' THEN 1 ELSE 0 END), 0),
                            COALESCE(MAX(timestamp_ms), 0)
                     FROM usage_logs{where_sql}
                     GROUP BY provider_name ORDER BY MAX(timestamp_ms) DESC, provider_name ASC"
                );
                let mut statement = query.prepare(&sql)?;
                let rows = statement
                    .query_map(params_from_iter(params.iter()), |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    })
                    .map_err(|error| error.to_string())?;
                for row in rows {
                    let (group, request_count, error_count, last_request_at_ms) =
                        row.map_err(|error| error.to_string())?;
                    groups.push(UsageLogGroupRow {
                        group,
                        request_count: request_count as u32,
                        error_count: error_count as u32,
                        last_request_at_ms,
                    });
                }
            } else {
                let sql = format!(
                    "SELECT local_model,
                            COUNT(*),
                            COALESCE(SUM(CASE WHEN result = 'failure' THEN 1 ELSE 0 END), 0),
                            COALESCE(MAX(timestamp_ms), 0)
                     FROM usage_logs{where_sql}
                     GROUP BY local_model ORDER BY MAX(timestamp_ms) DESC, local_model ASC"
                );
                let mut statement = query.prepare(&sql)?;
                let rows = statement
                    .query_map(params_from_iter(params.iter()), |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    })
                    .map_err(|error| error.to_string())?;
                for row in rows {
                    let (group, request_count, error_count, last_request_at_ms) =
                        row.map_err(|error| error.to_string())?;
                    groups.push(UsageLogGroupRow {
                        group,
                        request_count: request_count as u32,
                        error_count: error_count as u32,
                        last_request_at_ms,
                    });
                }
            }
            Ok(groups)
        })
    }

    pub(in crate::ai_gateway) fn usage_stats(
        &self,
        range: &TimeRange,
        hour_buckets: bool,
    ) -> Result<UsageStats, String> {
        self.query_connection(|query| {
            // Every aggregate below shares this one terminal-only filter.
            let (stats_where, params) = bind(range, &LogFilter::default(), true);
            let totals: UsageMetrics = query
                .prepare(&format!("SELECT {} FROM usage_logs{stats_where}", metric_columns()))?
                .query_row(params_from_iter(params.iter()), |row| {
                    metrics_from_row(row, 0)
                })
                .map_err(|error| error.to_string())?;

            let bucket_sql = if hour_buckets {
                "((timestamp_ms + 28800000) / 3600000) % 24"
            } else {
                "((timestamp_ms + 28800000) / 86400000)"
            };
            let mut statement = query.prepare(&format!(
                "SELECT {bucket_sql} AS bucket_key, {}
                 FROM usage_logs{stats_where}
                 GROUP BY bucket_key ORDER BY bucket_key ASC",
                metric_columns()
            ))?;
            let mut buckets = Vec::new();
            for row in statement
                .query_map(params_from_iter(params.iter()), |row| {
                    let key = row.get::<_, i64>(0)?;
                    Ok((key, metrics_from_row(row, 1)?))
                })
                .map_err(|error| error.to_string())?
            {
                let (key, metrics) = row.map_err(|error| error.to_string())?;
                let label = if hour_buckets {
                    format!("{key:02}:00")
                } else {
                    day_index_label(key)
                };
                buckets.push(UsageBucketRow { label, metrics });
            }

            // One grouped scan produces both the model aggregates and the
            // provider rows, replacing a second full scan plus a per-model scan
            // of every provider row. Rows without a provider id still count
            // toward their model but never become a blank provider row.
            let mut grouped_statement = query.prepare(&format!(
                "SELECT local_model, provider_id, provider_name, {}
                 FROM usage_logs{stats_where}
                 GROUP BY local_model, provider_id, provider_name
                 ORDER BY local_model ASC, provider_name ASC, provider_id ASC",
                metric_columns()
            ))?;
            let mut model_metrics: HashMap<String, UsageMetrics> = HashMap::new();
            let mut providers_by_model: HashMap<String, Vec<UsageProviderRow>> = HashMap::new();
            let mut provider_totals: HashMap<(String, String), UsageMetrics> = HashMap::new();
            let mut provider_models: HashMap<(String, String), Vec<UsageProviderModelRow>> =
                HashMap::new();
            for row in grouped_statement
                .query_map(params_from_iter(params.iter()), |row| {
                    let model = row.get::<_, String>(0)?;
                    let provider_id = row.get::<_, String>(1)?;
                    let provider_name = row.get::<_, String>(2)?;
                    Ok((model, provider_id, provider_name, metrics_from_row(row, 3)?))
                })
                .map_err(|error| error.to_string())?
            {
                let (model, provider_id, provider_name, metrics) =
                    row.map_err(|error| error.to_string())?;
                accumulate_metrics(model_metrics.entry(model.clone()).or_default(), &metrics);
                if !provider_id.is_empty() {
                    let provider_key = (provider_id.clone(), provider_name.clone());
                    accumulate_metrics(
                        provider_totals.entry(provider_key.clone()).or_default(),
                        &metrics,
                    );
                    provider_models
                        .entry(provider_key)
                        .or_default()
                        .push(UsageProviderModelRow {
                            local_model: model.clone(),
                            metrics,
                        });
                    providers_by_model
                        .entry(model)
                        .or_default()
                        .push(UsageProviderRow {
                            provider_id,
                            provider_name,
                            metrics,
                        });
                }
            }
            let mut model_rows: Vec<(String, UsageMetrics)> = model_metrics.into_iter().collect();
            // Preserve the existing presentation order: largest token total
            // first, then the local model id.
            model_rows.sort_by(|left, right| {
                right
                    .1
                    .total_tokens
                    .cmp(&left.1.total_tokens)
                    .then_with(|| left.0.cmp(&right.0))
            });
            let mut models = Vec::with_capacity(model_rows.len());
            for (local_model, mut metrics) in model_rows {
                derive_cache_rate(&mut metrics);
                let providers = providers_by_model.remove(&local_model).unwrap_or_default();
                models.push(UsageModelRow {
                    local_model,
                    metrics,
                    providers,
                });
            }

            // Provider rows keep the stored identity split: the summary is
            // ordered by total tokens, while each provider's models are ordered
            // by their own token totals and local model id. Every aggregate
            // comes from the same grouped scan above.
            let mut provider_rows: Vec<((String, String), UsageMetrics)> =
                provider_totals.into_iter().collect();
            provider_rows.sort_by(|left, right| {
                right
                    .1
                    .total_tokens
                    .cmp(&left.1.total_tokens)
                    .then_with(|| left.0.1.cmp(&right.0.1))
            });
            let mut providers = Vec::with_capacity(provider_rows.len());
            for (provider_key, mut metrics) in provider_rows {
                derive_cache_rate(&mut metrics);
                let mut provider_model_rows =
                    provider_models.remove(&provider_key).unwrap_or_default();
                provider_model_rows.sort_by(|left, right| {
                    right
                        .metrics
                        .total_tokens
                        .cmp(&left.metrics.total_tokens)
                        .then_with(|| left.local_model.cmp(&right.local_model))
                });
                providers.push(UsageProviderStatsRow {
                    provider_id: provider_key.0,
                    provider_name: provider_key.1,
                    metrics,
                    models: provider_model_rows,
                });
            }

            let mut unpriced_items = Vec::new();
            if totals.unpriced_count > 0 {
                let mut unpriced_statement = query.prepare(&format!(
                     "SELECT provider_id, provider_name, local_model, upstream_model, COUNT(*)
                      FROM usage_logs{stats_where} AND {UNPRICED_ROW_SQL}
                     GROUP BY provider_id, provider_name, local_model, upstream_model
                     ORDER BY COUNT(*) DESC, provider_name ASC, local_model ASC"
                ))?;
                for row in unpriced_statement
                    .query_map(params_from_iter(params.iter()), |row| {
                        Ok(UnpricedUsageItem {
                            provider_id: row.get(0)?,
                            provider_name: row.get(1)?,
                            local_model: row.get(2)?,
                            upstream_model: row.get(3)?,
                            count: row.get::<_, i64>(4)? as u32,
                        })
                    })
                    .map_err(|error| error.to_string())?
                {
                    unpriced_items.push(row.map_err(|error| error.to_string())?);
                }
            }

            Ok(UsageStats {
                granularity: if hour_buckets { "hour" } else { "day" }.to_string(),
                totals,
                buckets,
                models,
                providers,
                unpriced_items,
            })
        })
    }
}
