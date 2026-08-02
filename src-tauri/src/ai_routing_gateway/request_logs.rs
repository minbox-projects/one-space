use chrono::{DateTime, Days, NaiveDate, TimeZone, Utc};
use rusqlite::{params, params_from_iter, types::Value as SqlValue, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{btree_map::Entry, BTreeMap};

use super::{
    error::{GatewayError, GatewayErrorCategory},
    gateway_key::GatewayKeyGrant,
    pricing::{self, CostEstimate, TokenUsage},
    router::RouteCandidate,
    types::{AccountType, PriceSnapshot},
};

const MAX_PAGE_SIZE: u16 = 100;
const CLEANUP_BATCH_SIZE: i64 = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestLogDraft {
    pub(crate) id: String,
    pub(crate) request_id: String,
    pub(crate) started_at: String,
    pub(crate) local_date: String,
    pub(crate) timezone_name: String,
    pub(crate) endpoint: String,
    pub(crate) public_model_id: String,
    pub(crate) upstream_model_id_snapshot: Option<String>,
    pub(crate) api_key_id: String,
    pub(crate) api_key_name_snapshot: String,
    pub(crate) account_id: Option<String>,
    pub(crate) account_name_snapshot: Option<String>,
    pub(crate) group_id_snapshot: Option<String>,
    pub(crate) group_name_snapshot: Option<String>,
    pub(crate) price_snapshot: Option<PriceSnapshot>,
    pub(crate) cost_basis: CostBasis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CostBasis {
    PublicApiEquivalentEstimate,
    ThirdPartyConfiguredEstimate,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptDraft {
    pub(crate) id: String,
    pub(crate) attempt_number: u8,
    pub(crate) account_id: String,
    pub(crate) account_name_snapshot: String,
    pub(crate) group_id_snapshot: String,
    pub(crate) group_name_snapshot: String,
    pub(crate) upstream_model_id_snapshot: String,
    pub(crate) started_at: String,
    pub(crate) completed_at: String,
    pub(crate) status: AttemptStatus,
    pub(crate) error_code: Option<String>,
    pub(crate) emitted_client_bytes: bool,
    pub(crate) affected_health: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttemptStatus {
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

impl AttemptStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequestStatus {
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

impl RequestStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestCompletion {
    pub(crate) completed_at: String,
    pub(crate) status: RequestStatus,
    pub(crate) error_code: Option<String>,
    pub(crate) usage: TokenUsage,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LogFilters {
    pub(crate) started_at_or_after: Option<String>,
    pub(crate) started_before: Option<String>,
    pub(crate) account_id: Option<String>,
    pub(crate) group_id: Option<String>,
    pub(crate) public_model_id: Option<String>,
    pub(crate) upstream_model_id: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) error_code: Option<String>,
    pub(crate) api_key_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RequestLogRow {
    pub(crate) id: String,
    pub(crate) request_id: String,
    pub(crate) started_at: String,
    pub(crate) completed_at: Option<String>,
    pub(crate) local_date: String,
    pub(crate) timezone_name: String,
    pub(crate) endpoint: String,
    pub(crate) public_model_id: String,
    pub(crate) upstream_model_id_snapshot: Option<String>,
    pub(crate) api_key_id: Option<String>,
    pub(crate) api_key_name_snapshot: Option<String>,
    pub(crate) account_id: Option<String>,
    pub(crate) account_name_snapshot: Option<String>,
    pub(crate) group_id_snapshot: Option<String>,
    pub(crate) group_name_snapshot: Option<String>,
    pub(crate) status: String,
    pub(crate) error_code: Option<String>,
    pub(crate) usage: TokenUsage,
    pub(crate) estimated_cost_usd: Option<String>,
    pub(crate) cost_calculable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogPage {
    pub(crate) items: Vec<RequestLogRow>,
    pub(crate) next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AttemptRow {
    pub(crate) id: String,
    pub(crate) attempt_number: u8,
    pub(crate) account_id: Option<String>,
    pub(crate) account_name_snapshot: String,
    pub(crate) group_id_snapshot: Option<String>,
    pub(crate) group_name_snapshot: Option<String>,
    pub(crate) upstream_model_id_snapshot: Option<String>,
    pub(crate) started_at: String,
    pub(crate) completed_at: Option<String>,
    pub(crate) status: String,
    pub(crate) error_code: Option<String>,
    pub(crate) emitted_client_bytes: bool,
    pub(crate) affected_health: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetentionPolicy {
    Days7,
    Days30,
    Days90,
    Days180,
    Forever,
}

impl RetentionPolicy {
    fn days(self) -> Option<i64> {
        match self {
            Self::Days7 => Some(7),
            Self::Days30 => Some(30),
            Self::Days90 => Some(90),
            Self::Days180 => Some(180),
            Self::Forever => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TrendPoint {
    pub(crate) local_date: String,
    pub(crate) request_count: u64,
    pub(crate) success_count: u64,
    pub(crate) failure_count: u64,
    pub(crate) usage: TokenUsage,
    pub(crate) estimated_cost_usd: Option<String>,
    pub(crate) cost_calculable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AggregateValidation {
    pub(crate) expected_rows: usize,
    pub(crate) actual_rows: usize,
    pub(crate) mismatched_rows: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct AggregateKey {
    local_date: String,
    timezone_name: String,
    account_id: String,
    group_id: String,
    public_model_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AggregateValue {
    account_name: Option<String>,
    group_name: Option<String>,
    details_covered_through: Option<String>,
    request_count: u64,
    success_count: u64,
    failure_count: u64,
    usage: TokenUsage,
    estimated_cost_usd: Option<String>,
    cost_calculable: bool,
}

#[derive(Serialize)]
struct StoredPriceSnapshot<'a> {
    cost_basis: CostBasis,
    price: &'a PriceSnapshot,
}

pub(crate) fn begin_request<T: TimeZone>(
    connection: &Connection,
    grant: &GatewayKeyGrant,
    candidate: &RouteCandidate,
    endpoint: &str,
    public_model_id: &str,
    request_id: &str,
    started_at: DateTime<Utc>,
    local_time: DateTime<T>,
    timezone_name: &str,
) -> Result<RequestLogDraft, GatewayError>
where
    T::Offset: std::fmt::Display,
{
    validate_machine_value(request_id)?;
    validate_machine_value(endpoint)?;
    validate_machine_value(public_model_id)?;
    validate_machine_value(timezone_name)?;
    let group_name: String = connection
        .query_row(
            "SELECT name FROM ai_gateway_groups WHERE id = ?1",
            [&candidate.group_id],
            |row| row.get(0),
        )
        .map_err(|_| storage(Some(request_id)))?;
    let started_at_text = started_at.to_rfc3339();
    let price_snapshot = pricing::snapshot_price(
        connection,
        public_model_id,
        Some(&candidate.account_id),
        &started_at_text,
    )?;
    Ok(RequestLogDraft {
        id: uuid::Uuid::new_v4().to_string(),
        request_id: request_id.to_owned(),
        started_at: started_at_text,
        local_date: local_time.date_naive().to_string(),
        timezone_name: timezone_name.to_owned(),
        endpoint: endpoint.to_owned(),
        public_model_id: public_model_id.to_owned(),
        upstream_model_id_snapshot: Some(candidate.upstream_model.clone()),
        api_key_id: grant.id.clone(),
        api_key_name_snapshot: bounded_snapshot(&grant.name),
        account_id: Some(candidate.account_id.clone()),
        account_name_snapshot: Some(bounded_snapshot(&candidate.account_name)),
        group_id_snapshot: Some(candidate.group_id.clone()),
        group_name_snapshot: Some(bounded_snapshot(&group_name)),
        cost_basis: match candidate.account_type {
            AccountType::OAuth => CostBasis::PublicApiEquivalentEstimate,
            AccountType::ApiKey => CostBasis::ThirdPartyConfiguredEstimate,
        },
        price_snapshot,
    })
}

pub(crate) fn begin_unrouted_request<T: TimeZone>(
    grant: &GatewayKeyGrant,
    endpoint: &str,
    public_model_id: &str,
    request_id: &str,
    started_at: DateTime<Utc>,
    local_time: DateTime<T>,
    timezone_name: &str,
) -> Result<RequestLogDraft, GatewayError>
where
    T::Offset: std::fmt::Display,
{
    validate_machine_value(request_id)?;
    validate_machine_value(endpoint)?;
    validate_machine_value(public_model_id)?;
    validate_machine_value(timezone_name)?;
    Ok(RequestLogDraft {
        id: uuid::Uuid::new_v4().to_string(),
        request_id: request_id.to_owned(),
        started_at: started_at.to_rfc3339(),
        local_date: local_time.date_naive().to_string(),
        timezone_name: timezone_name.to_owned(),
        endpoint: endpoint.to_owned(),
        public_model_id: public_model_id.to_owned(),
        upstream_model_id_snapshot: None,
        api_key_id: grant.id.clone(),
        api_key_name_snapshot: bounded_snapshot(&grant.name),
        account_id: None,
        account_name_snapshot: None,
        group_id_snapshot: None,
        group_name_snapshot: None,
        price_snapshot: None,
        cost_basis: CostBasis::Unavailable,
    })
}

pub(crate) fn attempt(
    request: &RequestLogDraft,
    candidate: &RouteCandidate,
    attempt_number: u8,
    started_at: DateTime<Utc>,
    completed_at: DateTime<Utc>,
    status: AttemptStatus,
    error_code: Option<&str>,
    emitted_client_bytes: bool,
    affected_health: bool,
) -> Result<AttemptDraft, GatewayError> {
    if !(1..=6).contains(&attempt_number) {
        return Err(invalid(Some(&request.request_id)));
    }
    let error_code = validated_error_code(error_code)?;
    Ok(AttemptDraft {
        id: uuid::Uuid::new_v4().to_string(),
        attempt_number,
        account_id: candidate.account_id.clone(),
        account_name_snapshot: bounded_snapshot(&candidate.account_name),
        group_id_snapshot: candidate.group_id.clone(),
        group_name_snapshot: request.group_name_snapshot.clone().unwrap_or_default(),
        upstream_model_id_snapshot: candidate.upstream_model.clone(),
        started_at: started_at.to_rfc3339(),
        completed_at: completed_at.to_rfc3339(),
        status,
        error_code,
        emitted_client_bytes,
        affected_health,
    })
}

pub(crate) fn complete_request(
    connection: &mut Connection,
    request: &RequestLogDraft,
    attempts: &[AttemptDraft],
    completion: &RequestCompletion,
) -> Result<(), GatewayError> {
    if attempts.len() > 6 {
        return Err(invalid(Some(&request.request_id)));
    }
    validate_attempt_sequence(attempts, &request.request_id)?;
    let error_code = validated_error_code(completion.error_code.as_deref())?;
    let cost = pricing::estimate_cost(request.price_snapshot.as_ref(), completion.usage);
    let (estimated_cost, cost_calculable) = match cost {
        CostEstimate::Calculable(value) => (Some(value), true),
        CostEstimate::NotCalculable => (None, false),
    };
    let price_snapshot_json = request
        .price_snapshot
        .as_ref()
        .map(|price| {
            serde_json::to_string(&StoredPriceSnapshot {
                cost_basis: request.cost_basis,
                price,
            })
        })
        .transpose()
        .map_err(|_| invalid(Some(&request.request_id)))?;
    let transaction = connection
        .transaction()
        .map_err(|_| storage(Some(&request.request_id)))?;
    transaction
        .execute(
            "INSERT INTO ai_gateway_request_logs (id, request_id, started_at, completed_at, local_date, timezone_name, endpoint, public_model_id, upstream_model_id_snapshot, api_key_id, api_key_id_snapshot, api_key_name_snapshot, account_id, account_id_snapshot, account_name_snapshot, group_id_snapshot, group_name_snapshot, status, error_code, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, total_tokens, price_snapshot_json, estimated_cost_usd, cost_calculable) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)",
             params![
                request.id,
                request.request_id,
                request.started_at,
                completion.completed_at,
                request.local_date,
                request.timezone_name,
                request.endpoint,
                request.public_model_id,
                request.upstream_model_id_snapshot,
                request.api_key_id,
                request.api_key_id,
                request.api_key_name_snapshot,
                request.account_id,
                request.account_id,
                request.account_name_snapshot,
                request.group_id_snapshot,
                request.group_name_snapshot,
                completion.status.as_str(),
                error_code,
                to_sql_integer(completion.usage.input_tokens)?,
                to_sql_integer(completion.usage.output_tokens)?,
                to_sql_integer(completion.usage.cache_read_tokens)?,
                to_sql_integer(completion.usage.cache_write_tokens)?,
                to_sql_integer(resolved_total_tokens(completion.usage))?,
                price_snapshot_json,
                estimated_cost,
                cost_calculable,
            ],
        )
        .map_err(|_| storage(Some(&request.request_id)))?;
    for item in attempts {
        transaction
            .execute(
                "INSERT INTO ai_gateway_request_attempts (id, request_log_id, attempt_number, account_id, account_name_snapshot, group_id_snapshot, group_name_snapshot, upstream_model_id_snapshot, started_at, completed_at, status, error_code, emitted_client_bytes, affected_health) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![item.id, request.id, item.attempt_number, item.account_id, item.account_name_snapshot, item.group_id_snapshot, item.group_name_snapshot, item.upstream_model_id_snapshot, item.started_at, item.completed_at, item.status.as_str(), item.error_code, item.emitted_client_bytes, item.affected_health],
            )
            .map_err(|_| storage(Some(&request.request_id)))?;
    }
    let value = AggregateValue {
        account_name: request.account_name_snapshot.clone(),
        group_name: request.group_name_snapshot.clone(),
        details_covered_through: Some(request.started_at.clone()),
        request_count: 1,
        success_count: u64::from(completion.status == RequestStatus::Succeeded),
        failure_count: u64::from(completion.status != RequestStatus::Succeeded),
        usage: completion.usage,
        estimated_cost_usd: estimated_cost,
        cost_calculable,
    };
    merge_aggregate(
        &transaction,
        &AggregateKey {
            local_date: request.local_date.clone(),
            timezone_name: request.timezone_name.clone(),
            account_id: request.account_id.clone().unwrap_or_default(),
            group_id: request.group_id_snapshot.clone().unwrap_or_default(),
            public_model_id: request.public_model_id.clone(),
        },
        &value,
    )?;
    transaction
        .commit()
        .map_err(|_| storage(Some(&request.request_id)))
}

pub(crate) fn usage_from_response(value: &Value) -> TokenUsage {
    let usage = value.get("usage").or_else(|| {
        value
            .get("response")
            .and_then(|response| response.get("usage"))
    });
    let Some(usage) = usage else {
        return empty_usage();
    };
    let input_tokens = number(usage, &["input_tokens", "prompt_tokens"]);
    let output_tokens = number(usage, &["output_tokens", "completion_tokens"]);
    TokenUsage {
        input_tokens,
        output_tokens,
        cache_read_tokens: number(usage, &["cache_read_tokens"]).or_else(|| {
            usage
                .get("input_tokens_details")
                .or_else(|| usage.get("prompt_tokens_details"))
                .and_then(|details| number(details, &["cached_tokens", "cache_read_tokens"]))
        }),
        cache_write_tokens: number(usage, &["cache_write_tokens"]).or_else(|| {
            usage
                .get("input_tokens_details")
                .or_else(|| usage.get("prompt_tokens_details"))
                .and_then(|details| number(details, &["cache_write_tokens"]))
        }),
        total_tokens: number(usage, &["total_tokens"])
            .or_else(|| input_tokens?.checked_add(output_tokens?)),
    }
}

pub(crate) fn query_logs(
    connection: &Connection,
    filters: &LogFilters,
    cursor: Option<&str>,
    page_size: u16,
) -> Result<LogPage, GatewayError> {
    if page_size == 0 || page_size > MAX_PAGE_SIZE {
        return Err(invalid(None));
    }
    let mut sql = String::from(
        "SELECT id, request_id, started_at, completed_at, local_date, timezone_name, endpoint, public_model_id, upstream_model_id_snapshot, api_key_id_snapshot, api_key_name_snapshot, account_id_snapshot, account_name_snapshot, group_id_snapshot, group_name_snapshot, status, error_code, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, total_tokens, estimated_cost_usd, cost_calculable FROM ai_gateway_request_logs WHERE 1 = 1",
    );
    let mut values = Vec::<SqlValue>::new();
    push_filter(
        &mut sql,
        &mut values,
        "started_at >=",
        filters.started_at_or_after.as_deref(),
    );
    push_filter(
        &mut sql,
        &mut values,
        "started_at <",
        filters.started_before.as_deref(),
    );
    push_filter(
        &mut sql,
        &mut values,
        "account_id_snapshot =",
        filters.account_id.as_deref(),
    );
    push_filter(
        &mut sql,
        &mut values,
        "group_id_snapshot =",
        filters.group_id.as_deref(),
    );
    push_filter(
        &mut sql,
        &mut values,
        "public_model_id =",
        filters.public_model_id.as_deref(),
    );
    push_filter(
        &mut sql,
        &mut values,
        "upstream_model_id_snapshot =",
        filters.upstream_model_id.as_deref(),
    );
    push_filter(&mut sql, &mut values, "status =", filters.status.as_deref());
    push_filter(
        &mut sql,
        &mut values,
        "error_code =",
        filters.error_code.as_deref(),
    );
    push_filter(
        &mut sql,
        &mut values,
        "api_key_id_snapshot =",
        filters.api_key_id.as_deref(),
    );
    if let Some(cursor) = cursor {
        let (started_at, id) = decode_cursor(cursor)?;
        sql.push_str(" AND (started_at < ? OR (started_at = ? AND id < ?))");
        values.push(started_at.clone().into());
        values.push(started_at.into());
        values.push(id.into());
    }
    sql.push_str(" ORDER BY started_at DESC, id DESC LIMIT ?");
    values.push(SqlValue::Integer(i64::from(page_size) + 1));
    let mut statement = connection.prepare(&sql).map_err(|_| storage(None))?;
    let rows = statement
        .query_map(params_from_iter(values), map_log_row)
        .map_err(|_| storage(None))?;
    let mut items = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| storage(None))?;
    let has_more = items.len() > usize::from(page_size);
    items.truncate(usize::from(page_size));
    let next_cursor = has_more
        .then(|| {
            items
                .last()
                .map(|item| encode_cursor(&item.started_at, &item.id))
        })
        .flatten();
    Ok(LogPage { items, next_cursor })
}

pub(crate) fn query_attempts(
    connection: &Connection,
    request_log_id: &str,
) -> Result<Vec<AttemptRow>, GatewayError> {
    validate_machine_value(request_log_id)?;
    let mut statement = connection
        .prepare(
            "SELECT id, attempt_number, account_id, account_name_snapshot, group_id_snapshot, group_name_snapshot, upstream_model_id_snapshot, started_at, completed_at, status, error_code, emitted_client_bytes, affected_health FROM ai_gateway_request_attempts WHERE request_log_id = ?1 ORDER BY attempt_number",
        )
        .map_err(|_| storage(Some(request_log_id)))?;
    let rows = statement
        .query_map([request_log_id], |row| {
            let attempt_number = row.get::<_, i64>(1)?;
            Ok(AttemptRow {
                id: row.get(0)?,
                attempt_number: u8::try_from(attempt_number).unwrap_or(0),
                account_id: row.get(2)?,
                account_name_snapshot: row.get(3)?,
                group_id_snapshot: row.get(4)?,
                group_name_snapshot: row.get(5)?,
                upstream_model_id_snapshot: row.get(6)?,
                started_at: row.get(7)?,
                completed_at: row.get(8)?,
                status: row.get(9)?,
                error_code: row.get(10)?,
                emitted_client_bytes: row.get(11)?,
                affected_health: row.get(12)?,
            })
        })
        .map_err(|_| storage(Some(request_log_id)))?;
    rows.collect::<Result<_, _>>()
        .map_err(|_| storage(Some(request_log_id)))
}

pub(crate) fn set_retention_policy(
    connection: &Connection,
    policy: RetentionPolicy,
) -> Result<(), GatewayError> {
    connection
        .execute(
            "UPDATE ai_gateway_settings SET log_retention_days = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = 1",
            [policy.days()],
        )
        .map_err(|_| storage(None))?;
    Ok(())
}

pub(crate) fn trend(
    connection: &Connection,
    end_date: NaiveDate,
    days: u8,
    account_id: Option<&str>,
    group_id: Option<&str>,
    public_model_id: Option<&str>,
) -> Result<Vec<TrendPoint>, GatewayError> {
    if !matches!(days, 7 | 15 | 30) {
        return Err(invalid(None));
    }
    let start_date = end_date
        .checked_sub_days(Days::new(u64::from(days - 1)))
        .ok_or_else(|| invalid(None))?;
    let mut sql = String::from(
        "SELECT local_date, request_count, success_count, failure_count, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, total_tokens, estimated_cost_usd, cost_calculable FROM ai_gateway_daily_aggregates WHERE local_date >= ?1 AND local_date <= ?2",
    );
    let mut values = vec![
        SqlValue::Text(start_date.to_string()),
        SqlValue::Text(end_date.to_string()),
    ];
    push_filter(&mut sql, &mut values, "account_id_snapshot =", account_id);
    push_filter(&mut sql, &mut values, "group_id_snapshot =", group_id);
    push_filter(&mut sql, &mut values, "public_model_id =", public_model_id);
    let mut statement = connection.prepare(&sql).map_err(|_| storage(None))?;
    let rows = statement
        .query_map(params_from_iter(values), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                TokenUsage {
                    input_tokens: sql_u64(row.get(4)?),
                    output_tokens: sql_u64(row.get(5)?),
                    cache_read_tokens: sql_u64(row.get(6)?),
                    cache_write_tokens: sql_u64(row.get(7)?),
                    total_tokens: sql_u64(row.get(8)?),
                },
                row.get::<_, Option<String>>(9)?,
                row.get::<_, bool>(10)?,
            ))
        })
        .map_err(|_| storage(None))?;
    let mut grouped = BTreeMap::<String, TrendPoint>::new();
    for row in rows {
        let (date, requests, successes, failures, usage, cost, calculable) =
            row.map_err(|_| storage(None))?;
        let point = match grouped.entry(date.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(TrendPoint {
                    local_date: date,
                    request_count: requests.max(0) as u64,
                    success_count: successes.max(0) as u64,
                    failure_count: failures.max(0) as u64,
                    usage,
                    estimated_cost_usd: calculable.then_some(cost.clone()).flatten(),
                    cost_calculable: calculable && cost.is_some(),
                });
                continue;
            }
            Entry::Occupied(entry) => entry.into_mut(),
        };
        point.request_count = point.request_count.saturating_add(requests.max(0) as u64);
        point.success_count = point.success_count.saturating_add(successes.max(0) as u64);
        point.failure_count = point.failure_count.saturating_add(failures.max(0) as u64);
        point.usage = sum_usage(point.usage, usage);
        if !calculable || cost.is_none() {
            point.cost_calculable = false;
            point.estimated_cost_usd = None;
        } else if point.cost_calculable {
            point.estimated_cost_usd =
                add_costs(point.estimated_cost_usd.as_deref(), cost.as_deref());
            if point.estimated_cost_usd.is_none() {
                point.cost_calculable = false;
            }
        }
    }
    let mut output = Vec::with_capacity(usize::from(days));
    for offset in 0..days {
        let date = start_date
            .checked_add_days(Days::new(u64::from(offset)))
            .ok_or_else(|| invalid(None))?;
        output.push(grouped.remove(&date.to_string()).unwrap_or(TrendPoint {
            local_date: date.to_string(),
            request_count: 0,
            success_count: 0,
            failure_count: 0,
            usage: TokenUsage {
                input_tokens: Some(0),
                output_tokens: Some(0),
                cache_read_tokens: Some(0),
                cache_write_tokens: Some(0),
                total_tokens: Some(0),
            },
            estimated_cost_usd: Some("0".into()),
            cost_calculable: true,
        }));
    }
    Ok(output)
}

pub(crate) fn cleanup_retained_details(
    connection: &mut Connection,
    now: DateTime<Utc>,
) -> Result<usize, GatewayError> {
    let retention: Option<i64> = connection
        .query_row(
            "SELECT log_retention_days FROM ai_gateway_settings WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|_| storage(None))?;
    let Some(days) = retention else {
        return Ok(0);
    };
    let cutoff = now - chrono::Duration::days(days);
    let transaction = connection.transaction().map_err(|_| storage(None))?;
    let affected_dates: Vec<String> = {
        let mut statement = transaction
            .prepare(
                "SELECT DISTINCT local_date FROM ai_gateway_request_logs WHERE started_at < ?1 ORDER BY local_date LIMIT ?2",
            )
            .map_err(|_| storage(None))?;
        let rows = statement
            .query_map(params![cutoff.to_rfc3339(), CLEANUP_BATCH_SIZE], |row| {
                row.get(0)
            })
            .map_err(|_| storage(None))?;
        rows.collect::<Result<_, _>>().map_err(|_| storage(None))?
    };
    for local_date in affected_dates {
        transaction
            .execute(
                "UPDATE ai_gateway_daily_aggregates SET details_covered_through = NULL, updated_at = CURRENT_TIMESTAMP WHERE local_date = ?1",
                [local_date],
            )
            .map_err(|_| storage(None))?;
    }
    let deleted = transaction
        .execute(
            "DELETE FROM ai_gateway_request_logs WHERE id IN (SELECT id FROM ai_gateway_request_logs WHERE started_at < ?1 ORDER BY started_at LIMIT ?2)",
            params![cutoff.to_rfc3339(), CLEANUP_BATCH_SIZE],
        )
        .map_err(|_| storage(None))?;
    transaction.commit().map_err(|_| storage(None))?;
    Ok(deleted)
}

pub(crate) fn clear_details(connection: &mut Connection) -> Result<usize, GatewayError> {
    let transaction = connection.transaction().map_err(|_| storage(None))?;
    transaction
        .execute(
            "UPDATE ai_gateway_daily_aggregates SET details_covered_through = NULL, updated_at = CURRENT_TIMESTAMP",
            [],
        )
        .map_err(|_| storage(None))?;
    let deleted = transaction
        .execute("DELETE FROM ai_gateway_request_logs", [])
        .map_err(|_| storage(None))?;
    transaction.commit().map_err(|_| storage(None))?;
    Ok(deleted)
}

pub(crate) fn run_sqlite_maintenance(connection: &Connection) -> Result<(), GatewayError> {
    connection
        .execute_batch(
            "PRAGMA wal_checkpoint(PASSIVE); PRAGMA optimize; PRAGMA incremental_vacuum(200);",
        )
        .map_err(|_| storage(None))
}

pub(crate) fn rebuild_aggregates(
    connection: &mut Connection,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<usize, GatewayError> {
    if start_date > end_date {
        return Err(invalid(None));
    }
    let expected = aggregates_from_logs(connection, start_date, end_date)?;
    let existing = load_aggregates(connection, start_date, end_date)?;
    let transaction = connection.transaction().map_err(|_| storage(None))?;
    for (key, value) in &existing {
        if value.details_covered_through.is_some() && !expected.contains_key(key) {
            delete_aggregate(&transaction, key)?;
        }
    }
    for (key, value) in &expected {
        if existing
            .get(key)
            .is_some_and(|aggregate| aggregate.details_covered_through.is_none())
        {
            continue;
        }
        replace_aggregate(&transaction, key, value)?;
    }
    transaction.commit().map_err(|_| storage(None))?;
    Ok(expected.len())
}

pub(crate) fn validate_aggregates(
    connection: &Connection,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<AggregateValidation, GatewayError> {
    if start_date > end_date {
        return Err(invalid(None));
    }
    let expected = aggregates_from_logs(connection, start_date, end_date)?;
    let actual = load_aggregates(connection, start_date, end_date)?;
    let mismatched_rows = expected
        .iter()
        .filter(|(key, value)| match actual.get(*key) {
            None => true,
            Some(actual) if actual.details_covered_through.is_some() => actual != *value,
            Some(_) => false,
        })
        .count()
        + actual
            .keys()
            .filter(|key| {
                actual
                    .get(*key)
                    .is_some_and(|value| value.details_covered_through.is_some())
                    && !expected.contains_key(*key)
            })
            .count();
    let expected_rows = expected
        .iter()
        .filter(|(key, _)| {
            actual
                .get(*key)
                .is_none_or(|value| value.details_covered_through.is_some())
        })
        .count();
    let actual_rows = actual
        .values()
        .filter(|value| value.details_covered_through.is_some())
        .count();
    Ok(AggregateValidation {
        expected_rows,
        actual_rows,
        mismatched_rows,
    })
}

fn aggregates_from_logs(
    connection: &Connection,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<BTreeMap<AggregateKey, AggregateValue>, GatewayError> {
    let mut statement = connection
        .prepare(
            "SELECT local_date, timezone_name, COALESCE(account_id_snapshot, account_id, ''), account_name_snapshot, COALESCE(group_id_snapshot, ''), group_name_snapshot, public_model_id, status, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, total_tokens, estimated_cost_usd, cost_calculable, started_at FROM ai_gateway_request_logs WHERE local_date >= ?1 AND local_date <= ?2 ORDER BY local_date, id",
        )
        .map_err(|_| storage(None))?;
    let rows = statement
        .query_map(
            params![start_date.to_string(), end_date.to_string()],
            |row| {
                Ok((
                    AggregateKey {
                        local_date: row.get(0)?,
                        timezone_name: row.get(1)?,
                        account_id: row.get(2)?,
                        group_id: row.get(4)?,
                        public_model_id: row.get(6)?,
                    },
                    AggregateValue {
                        account_name: row.get(3)?,
                        group_name: row.get(5)?,
                        details_covered_through: Some(row.get(15)?),
                        request_count: 1,
                        success_count: u64::from(row.get::<_, String>(7)? == "succeeded"),
                        failure_count: 0,
                        usage: TokenUsage {
                            input_tokens: sql_u64(row.get(8)?),
                            output_tokens: sql_u64(row.get(9)?),
                            cache_read_tokens: sql_u64(row.get(10)?),
                            cache_write_tokens: sql_u64(row.get(11)?),
                            total_tokens: sql_u64(row.get(12)?),
                        },
                        estimated_cost_usd: row.get(13)?,
                        cost_calculable: row.get(14)?,
                    },
                ))
            },
        )
        .map_err(|_| storage(None))?;
    let mut output = BTreeMap::new();
    for row in rows {
        let (key, mut value) = row.map_err(|_| storage(None))?;
        value.failure_count = value.request_count - value.success_count;
        match output.get_mut(&key) {
            Some(existing) => combine_aggregate(existing, &value),
            None => {
                output.insert(key, value);
            }
        }
    }
    Ok(output)
}

fn load_aggregates(
    connection: &Connection,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<BTreeMap<AggregateKey, AggregateValue>, GatewayError> {
    let mut statement = connection
        .prepare(
            "SELECT local_date, timezone_name, account_id_snapshot, account_name_snapshot, group_id_snapshot, group_name_snapshot, public_model_id, request_count, success_count, failure_count, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, total_tokens, estimated_cost_usd, cost_calculable, details_covered_through FROM ai_gateway_daily_aggregates WHERE local_date >= ?1 AND local_date <= ?2",
        )
        .map_err(|_| storage(None))?;
    let rows = statement
        .query_map(
            params![start_date.to_string(), end_date.to_string()],
            |row| {
                Ok((
                    AggregateKey {
                        local_date: row.get(0)?,
                        timezone_name: row.get(1)?,
                        account_id: row.get(2)?,
                        group_id: row.get(4)?,
                        public_model_id: row.get(6)?,
                    },
                    AggregateValue {
                        account_name: row.get(3)?,
                        group_name: row.get(5)?,
                        details_covered_through: row.get(17)?,
                        request_count: sql_count(row.get(7)?),
                        success_count: sql_count(row.get(8)?),
                        failure_count: sql_count(row.get(9)?),
                        usage: TokenUsage {
                            input_tokens: sql_u64(row.get(10)?),
                            output_tokens: sql_u64(row.get(11)?),
                            cache_read_tokens: sql_u64(row.get(12)?),
                            cache_write_tokens: sql_u64(row.get(13)?),
                            total_tokens: sql_u64(row.get(14)?),
                        },
                        estimated_cost_usd: row.get(15)?,
                        cost_calculable: row.get(16)?,
                    },
                ))
            },
        )
        .map_err(|_| storage(None))?;
    rows.collect::<Result<_, _>>().map_err(|_| storage(None))
}

fn merge_aggregate(
    connection: &Connection,
    key: &AggregateKey,
    incoming: &AggregateValue,
) -> Result<(), GatewayError> {
    let existing = connection
        .query_row(
            "SELECT account_name_snapshot, group_name_snapshot, request_count, success_count, failure_count, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, total_tokens, estimated_cost_usd, cost_calculable, details_covered_through FROM ai_gateway_daily_aggregates WHERE local_date = ?1 AND timezone_name = ?2 AND account_id_snapshot = ?3 AND group_id_snapshot = ?4 AND public_model_id = ?5",
            params![key.local_date, key.timezone_name, key.account_id, key.group_id, key.public_model_id],
            |row| Ok(AggregateValue {
                account_name: row.get(0)?, group_name: row.get(1)?, details_covered_through: row.get(12)?, request_count: sql_count(row.get(2)?), success_count: sql_count(row.get(3)?), failure_count: sql_count(row.get(4)?),
                usage: TokenUsage { input_tokens: sql_u64(row.get(5)?), output_tokens: sql_u64(row.get(6)?), cache_read_tokens: sql_u64(row.get(7)?), cache_write_tokens: sql_u64(row.get(8)?), total_tokens: sql_u64(row.get(9)?) },
                estimated_cost_usd: row.get(10)?, cost_calculable: row.get(11)?,
            }),
        )
        .optional()
        .map_err(|_| storage(None))?;
    let merged = match existing {
        Some(mut existing) => {
            combine_aggregate(&mut existing, incoming);
            existing
        }
        None => incoming.clone(),
    };
    store_aggregate(connection, key, &merged)
}

fn replace_aggregate(
    connection: &Connection,
    key: &AggregateKey,
    value: &AggregateValue,
) -> Result<(), GatewayError> {
    store_aggregate(connection, key, value)
}

fn delete_aggregate(connection: &Connection, key: &AggregateKey) -> Result<(), GatewayError> {
    connection
        .execute(
            "DELETE FROM ai_gateway_daily_aggregates WHERE local_date = ?1 AND timezone_name = ?2 AND account_id_snapshot = ?3 AND group_id_snapshot = ?4 AND public_model_id = ?5 AND details_covered_through IS NOT NULL",
            params![key.local_date, key.timezone_name, key.account_id, key.group_id, key.public_model_id],
        )
        .map_err(|_| storage(None))?;
    Ok(())
}

fn store_aggregate(
    connection: &Connection,
    key: &AggregateKey,
    value: &AggregateValue,
) -> Result<(), GatewayError> {
    connection
        .execute(
            "INSERT INTO ai_gateway_daily_aggregates (local_date, timezone_name, account_id_snapshot, account_name_snapshot, group_id_snapshot, group_name_snapshot, public_model_id, request_count, success_count, failure_count, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, total_tokens, estimated_cost_usd, cost_calculable, details_covered_through, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, CURRENT_TIMESTAMP) ON CONFLICT(local_date, timezone_name, account_id_snapshot, group_id_snapshot, public_model_id) DO UPDATE SET account_name_snapshot = excluded.account_name_snapshot, group_name_snapshot = excluded.group_name_snapshot, request_count = excluded.request_count, success_count = excluded.success_count, failure_count = excluded.failure_count, input_tokens = excluded.input_tokens, output_tokens = excluded.output_tokens, cache_read_tokens = excluded.cache_read_tokens, cache_write_tokens = excluded.cache_write_tokens, total_tokens = excluded.total_tokens, estimated_cost_usd = excluded.estimated_cost_usd, cost_calculable = excluded.cost_calculable, details_covered_through = excluded.details_covered_through, updated_at = CURRENT_TIMESTAMP",
            params![key.local_date, key.timezone_name, key.account_id, value.account_name, key.group_id, value.group_name, key.public_model_id, to_sql_integer(Some(value.request_count))?, to_sql_integer(Some(value.success_count))?, to_sql_integer(Some(value.failure_count))?, to_sql_integer(value.usage.input_tokens)?, to_sql_integer(value.usage.output_tokens)?, to_sql_integer(value.usage.cache_read_tokens)?, to_sql_integer(value.usage.cache_write_tokens)?, to_sql_integer(resolved_total_tokens(value.usage))?, value.estimated_cost_usd, value.cost_calculable, value.details_covered_through],
        )
        .map_err(|_| storage(None))?;
    Ok(())
}

fn combine_aggregate(target: &mut AggregateValue, incoming: &AggregateValue) {
    target.details_covered_through = match (
        target.details_covered_through.as_deref(),
        incoming.details_covered_through.as_deref(),
    ) {
        (Some(left), Some(right)) => Some(left.max(right).to_owned()),
        _ => None,
    };
    target.request_count = target.request_count.saturating_add(incoming.request_count);
    target.success_count = target.success_count.saturating_add(incoming.success_count);
    target.failure_count = target.failure_count.saturating_add(incoming.failure_count);
    target.usage = sum_usage(target.usage, incoming.usage);
    if !target.cost_calculable || !incoming.cost_calculable {
        target.cost_calculable = false;
        target.estimated_cost_usd = None;
    } else {
        target.estimated_cost_usd = add_costs(
            target.estimated_cost_usd.as_deref(),
            incoming.estimated_cost_usd.as_deref(),
        );
        if target.estimated_cost_usd.is_none() {
            target.cost_calculable = false;
        }
    }
}

fn map_log_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RequestLogRow> {
    Ok(RequestLogRow {
        id: row.get(0)?,
        request_id: row.get(1)?,
        started_at: row.get(2)?,
        completed_at: row.get(3)?,
        local_date: row.get(4)?,
        timezone_name: row.get(5)?,
        endpoint: row.get(6)?,
        public_model_id: row.get(7)?,
        upstream_model_id_snapshot: row.get(8)?,
        api_key_id: row.get(9)?,
        api_key_name_snapshot: row.get(10)?,
        account_id: row.get(11)?,
        account_name_snapshot: row.get(12)?,
        group_id_snapshot: row.get(13)?,
        group_name_snapshot: row.get(14)?,
        status: row.get(15)?,
        error_code: row.get(16)?,
        usage: TokenUsage {
            input_tokens: sql_u64(row.get(17)?),
            output_tokens: sql_u64(row.get(18)?),
            cache_read_tokens: sql_u64(row.get(19)?),
            cache_write_tokens: sql_u64(row.get(20)?),
            total_tokens: sql_u64(row.get(21)?),
        },
        estimated_cost_usd: row.get(22)?,
        cost_calculable: row.get(23)?,
    })
}

fn push_filter(
    sql: &mut String,
    values: &mut Vec<SqlValue>,
    expression: &str,
    value: Option<&str>,
) {
    if let Some(value) = value {
        sql.push_str(" AND ");
        sql.push_str(expression);
        sql.push_str(" ?");
        values.push(SqlValue::Text(value.to_owned()));
    }
}

fn encode_cursor(started_at: &str, id: &str) -> String {
    format!("{}|{}", started_at.len(), started_at) + id
}

fn decode_cursor(cursor: &str) -> Result<(String, String), GatewayError> {
    let (length, rest) = cursor.split_once('|').ok_or_else(|| invalid(None))?;
    let length = length.parse::<usize>().map_err(|_| invalid(None))?;
    if rest.len() <= length || !rest.is_char_boundary(length) {
        return Err(invalid(None));
    }
    let (started_at, id) = rest.split_at(length);
    validate_machine_value(id)?;
    DateTime::parse_from_rfc3339(started_at).map_err(|_| invalid(None))?;
    Ok((started_at.to_owned(), id.to_owned()))
}

fn validate_attempt_sequence(
    attempts: &[AttemptDraft],
    request_id: &str,
) -> Result<(), GatewayError> {
    for (index, item) in attempts.iter().enumerate() {
        if usize::from(item.attempt_number) != index + 1 {
            return Err(invalid(Some(request_id)));
        }
    }
    Ok(())
}

fn validated_error_code(value: Option<&str>) -> Result<Option<String>, GatewayError> {
    value
        .map(|value| {
            validate_machine_value(value)?;
            if value.len() > 64 {
                return Err(invalid(None));
            }
            Ok(value.to_owned())
        })
        .transpose()
}

fn validate_machine_value(value: &str) -> Result<(), GatewayError> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'/' | b'.' | b':' | b'+')
        })
    {
        return Err(invalid(None));
    }
    Ok(())
}

fn bounded_snapshot(value: &str) -> String {
    value.chars().take(128).collect()
}

fn number(value: &Value, names: &[&str]) -> Option<u64> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_u64))
}

fn empty_usage() -> TokenUsage {
    TokenUsage {
        input_tokens: None,
        output_tokens: None,
        cache_read_tokens: None,
        cache_write_tokens: None,
        total_tokens: None,
    }
}

fn sum_usage(left: TokenUsage, right: TokenUsage) -> TokenUsage {
    TokenUsage {
        input_tokens: sum_optional(left.input_tokens, right.input_tokens),
        output_tokens: sum_optional(left.output_tokens, right.output_tokens),
        cache_read_tokens: sum_optional(left.cache_read_tokens, right.cache_read_tokens),
        cache_write_tokens: sum_optional(left.cache_write_tokens, right.cache_write_tokens),
        total_tokens: sum_optional(left.total_tokens, right.total_tokens),
    }
}

fn sum_optional(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => left.checked_add(right),
        (Some(_), None) | (None, Some(_)) => None,
        (None, None) => None,
    }
}

fn resolved_total_tokens(usage: TokenUsage) -> Option<u64> {
    usage
        .total_tokens
        .or_else(|| usage.input_tokens?.checked_add(usage.output_tokens?))
}

fn to_sql_integer(value: Option<u64>) -> Result<Option<i64>, GatewayError> {
    value
        .map(|value| i64::try_from(value).map_err(|_| invalid(None)))
        .transpose()
}

fn sql_u64(value: Option<i64>) -> Option<u64> {
    value.and_then(|value| u64::try_from(value).ok())
}

fn sql_count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn add_costs(left: Option<&str>, right: Option<&str>) -> Option<String> {
    let left = parse_decimal(left?)?;
    let right = parse_decimal(right?)?;
    format_decimal(left.checked_add(right)?)
}

fn parse_decimal(value: &str) -> Option<u128> {
    let mut parts = value.split('.');
    let whole = parts.next()?;
    let fraction = parts.next().unwrap_or("");
    if parts.next().is_some()
        || whole.is_empty()
        || fraction.len() > 9
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let whole = whole.parse::<u128>().ok()?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u128>().ok()? * 10u128.pow((9 - fraction.len()) as u32)
    };
    whole.checked_mul(1_000_000_000)?.checked_add(fraction)
}

fn format_decimal(value: u128) -> Option<String> {
    let whole = value / 1_000_000_000;
    let fraction = value % 1_000_000_000;
    if fraction == 0 {
        return Some(whole.to_string());
    }
    Some(
        format!("{whole}.{fraction:09}")
            .trim_end_matches('0')
            .to_owned(),
    )
}

fn invalid(entity_id: Option<&str>) -> GatewayError {
    GatewayError::new(GatewayErrorCategory::InvalidInput, entity_id)
}

fn storage(entity_id: Option<&str>) -> GatewayError {
    GatewayError::new(GatewayErrorCategory::StorageUnavailable, entity_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ai_routing_gateway::{
            gateway_key::GatewayKeyGrant, router::RouteCandidate, types::UpstreamProtocol,
        },
        shared_sqlite,
    };
    use chrono_tz::Tz;

    fn database(name: &str) -> (std::path::PathBuf, Connection) {
        let path = std::env::temp_dir().join(format!(
            "onespace-request-logs-{name}-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let connection = shared_sqlite::open_at(&path).unwrap();
        connection.execute("INSERT INTO ai_gateway_accounts (id, account_type, name, group_id) VALUES ('account-log', 'oauth', 'OAuth Account', 'default')", []).unwrap();
        connection.execute("INSERT INTO ai_gateway_api_keys (id, name, key_prefix, key_hash, hash_salt) VALUES ('key-log', 'CLI Key', 'osk_log', X'01', X'02')", []).unwrap();
        (path, connection)
    }

    fn cleanup(path: &std::path::Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    fn candidate() -> RouteCandidate {
        RouteCandidate {
            account_id: "account-log".into(),
            account_name: "OAuth Account".into(),
            group_id: "default".into(),
            account_type: AccountType::OAuth,
            base_url: "http://127.0.0.1:1/v1".into(),
            auth_method: "bearer".into(),
            protocol: UpstreamProtocol::Responses,
            upstream_model: "vendor-model".into(),
            sort_order: 0,
            quota_fresh: true,
            minimum_remaining_percent: None,
            last_used_at: None,
            is_probe: false,
        }
    }

    fn draft(
        connection: &Connection,
        request_id: &str,
        utc: DateTime<Utc>,
        zone: Tz,
    ) -> RequestLogDraft {
        begin_request(
            connection,
            &GatewayKeyGrant {
                id: "key-log".into(),
                name: "CLI Key".into(),
                group_ids: vec!["default".into()],
                model_ids: vec!["gpt-5.6-sol".into()],
            },
            &candidate(),
            "responses",
            "gpt-5.6-sol",
            request_id,
            utc,
            utc.with_timezone(&zone),
            zone.name(),
        )
        .unwrap()
    }

    fn finish(
        connection: &mut Connection,
        request: &RequestLogDraft,
        status: RequestStatus,
        usage: TokenUsage,
    ) {
        let candidate = candidate();
        let started = DateTime::parse_from_rfc3339(&request.started_at)
            .unwrap()
            .with_timezone(&Utc);
        let attempt = attempt(
            request,
            &candidate,
            1,
            started,
            started + chrono::Duration::seconds(1),
            if status == RequestStatus::Succeeded {
                AttemptStatus::Succeeded
            } else {
                AttemptStatus::Failed
            },
            (status != RequestStatus::Succeeded).then_some("upstream_unavailable"),
            false,
            status != RequestStatus::Succeeded,
        )
        .unwrap();
        complete_request(
            connection,
            request,
            &[attempt],
            &RequestCompletion {
                completed_at: (started + chrono::Duration::seconds(1)).to_rfc3339(),
                status,
                error_code: (status != RequestStatus::Succeeded)
                    .then(|| "upstream_unavailable".into()),
                usage,
            },
        )
        .unwrap();
    }

    #[test]
    fn completion_is_atomic_preserves_snapshots_usage_and_unknown_cost() {
        let (path, mut connection) = database("atomic");
        let utc = Utc.with_ymd_and_hms(2026, 11, 1, 5, 30, 0).unwrap();
        let request = draft(&connection, "req_atomic", utc, chrono_tz::America::New_York);
        finish(
            &mut connection,
            &request,
            RequestStatus::Succeeded,
            TokenUsage {
                input_tokens: Some(10),
                output_tokens: Some(4),
                cache_read_tokens: None,
                cache_write_tokens: None,
                total_tokens: Some(14),
            },
        );
        let row = query_logs(&connection, &LogFilters::default(), None, 20)
            .unwrap()
            .items
            .remove(0);
        assert_eq!(row.local_date, "2026-11-01");
        assert_eq!(row.timezone_name, "America/New_York");
        assert_eq!(row.usage.input_tokens, Some(10));
        assert_eq!(row.usage.total_tokens, Some(14));
        let stored_attempts = query_attempts(&connection, &row.id).unwrap();
        assert_eq!(stored_attempts.len(), 1);
        assert_eq!(stored_attempts[0].status, "succeeded");
        assert!(!row.cost_calculable);
        assert_eq!(row.estimated_cost_usd, None);
        connection
            .execute(
                "DELETE FROM ai_gateway_accounts WHERE id = 'account-log'",
                [],
            )
            .unwrap();
        connection
            .execute("DELETE FROM ai_gateway_api_keys WHERE id = 'key-log'", [])
            .unwrap();
        let snapshot = query_logs(&connection, &LogFilters::default(), None, 20)
            .unwrap()
            .items
            .remove(0);
        assert_eq!(snapshot.account_id.as_deref(), Some("account-log"));
        assert_eq!(
            snapshot.account_name_snapshot.as_deref(),
            Some("OAuth Account")
        );
        assert_eq!(snapshot.api_key_name_snapshot.as_deref(), Some("CLI Key"));
        assert_eq!(snapshot.api_key_id.as_deref(), Some("key-log"));
        let filtered_after_deletion = query_logs(
            &connection,
            &LogFilters {
                account_id: Some("account-log".into()),
                api_key_id: Some("key-log".into()),
                ..LogFilters::default()
            },
            None,
            20,
        )
        .unwrap();
        assert_eq!(filtered_after_deletion.items.len(), 1);
        cleanup(&path);
    }

    #[test]
    fn filters_cursor_and_dst_dates_are_stable() {
        let (path, mut connection) = database("pagination");
        let zone = chrono_tz::America::New_York;
        for (index, utc) in [
            Utc.with_ymd_and_hms(2026, 11, 1, 5, 30, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 11, 1, 6, 30, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 11, 2, 5, 30, 0).unwrap(),
        ]
        .into_iter()
        .enumerate()
        {
            let request = draft(&connection, &format!("req_page_{index}"), utc, zone);
            finish(
                &mut connection,
                &request,
                if index == 1 {
                    RequestStatus::Failed
                } else {
                    RequestStatus::Succeeded
                },
                TokenUsage {
                    input_tokens: Some(1),
                    output_tokens: Some(1),
                    cache_read_tokens: Some(0),
                    cache_write_tokens: Some(0),
                    total_tokens: Some(2),
                },
            );
        }
        let first = query_logs(&connection, &LogFilters::default(), None, 2).unwrap();
        assert_eq!(first.items.len(), 2);
        let second = query_logs(
            &connection,
            &LogFilters::default(),
            first.next_cursor.as_deref(),
            2,
        )
        .unwrap();
        assert_eq!(second.items.len(), 1);
        let failed = query_logs(
            &connection,
            &LogFilters {
                status: Some("failed".into()),
                error_code: Some("upstream_unavailable".into()),
                account_id: Some("account-log".into()),
                group_id: Some("default".into()),
                public_model_id: Some("gpt-5.6-sol".into()),
                upstream_model_id: Some("vendor-model".into()),
                api_key_id: Some("key-log".into()),
                ..LogFilters::default()
            },
            None,
            10,
        )
        .unwrap();
        assert_eq!(failed.items.len(), 1);
        assert_eq!(failed.items[0].local_date, "2026-11-01");
        cleanup(&path);
    }

    #[test]
    fn trend_zero_fill_preserves_unknown_cost_and_rebuild_detects_corruption() {
        let (path, mut connection) = database("trend");
        let zone = chrono_tz::UTC;
        let first = draft(
            &connection,
            "req_trend_1",
            Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap(),
            zone,
        );
        finish(
            &mut connection,
            &first,
            RequestStatus::Succeeded,
            TokenUsage {
                input_tokens: Some(1_000_000),
                output_tokens: Some(1_000_000),
                cache_read_tokens: Some(0),
                cache_write_tokens: Some(0),
                total_tokens: Some(2_000_000),
            },
        );
        let unknown = draft(
            &connection,
            "req_trend_2",
            Utc.with_ymd_and_hms(2026, 8, 3, 12, 0, 0).unwrap(),
            zone,
        );
        finish(
            &mut connection,
            &unknown,
            RequestStatus::Succeeded,
            TokenUsage {
                input_tokens: None,
                output_tokens: Some(1),
                cache_read_tokens: None,
                cache_write_tokens: None,
                total_tokens: Some(1),
            },
        );
        let points = trend(
            &connection,
            NaiveDate::from_ymd_opt(2026, 8, 7).unwrap(),
            7,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(points[1].request_count, 0);
        assert_eq!(points[1].estimated_cost_usd.as_deref(), Some("0"));
        assert_eq!(points[0].usage.input_tokens, Some(1_000_000));
        assert_eq!(points[0].usage.output_tokens, Some(1_000_000));
        assert_eq!(points[0].usage.total_tokens, Some(2_000_000));
        assert!(!points[2].cost_calculable);
        assert_eq!(points[2].estimated_cost_usd, None);
        connection.execute("UPDATE ai_gateway_daily_aggregates SET request_count = 99 WHERE local_date = '2026-08-01'", []).unwrap();
        assert!(
            validate_aggregates(
                &connection,
                NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 8, 7).unwrap()
            )
            .unwrap()
            .mismatched_rows
                > 0
        );
        rebuild_aggregates(
            &mut connection,
            NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 8, 7).unwrap(),
        )
        .unwrap();
        assert_eq!(
            validate_aggregates(
                &connection,
                NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 8, 7).unwrap()
            )
            .unwrap()
            .mismatched_rows,
            0
        );
        cleanup(&path);
    }

    #[test]
    fn retention_clear_rollback_and_aggregates_are_independent() {
        let (path, mut connection) = database("maintenance");
        for (policy, expected) in [
            (RetentionPolicy::Days7, Some(7)),
            (RetentionPolicy::Days30, Some(30)),
            (RetentionPolicy::Days90, Some(90)),
            (RetentionPolicy::Days180, Some(180)),
            (RetentionPolicy::Forever, None),
        ] {
            set_retention_policy(&connection, policy).unwrap();
            let stored: Option<i64> = connection
                .query_row(
                    "SELECT log_retention_days FROM ai_gateway_settings WHERE id = 1",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(stored, expected);
        }
        set_retention_policy(&connection, RetentionPolicy::Days90).unwrap();
        let request = draft(
            &connection,
            "req_old",
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            chrono_tz::UTC,
        );
        finish(
            &mut connection,
            &request,
            RequestStatus::Succeeded,
            TokenUsage {
                input_tokens: Some(1),
                output_tokens: Some(1),
                cache_read_tokens: Some(0),
                cache_write_tokens: Some(0),
                total_tokens: Some(2),
            },
        );
        assert_eq!(
            cleanup_retained_details(
                &mut connection,
                Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap()
            )
            .unwrap(),
            1
        );
        let aggregate_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_daily_aggregates",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(aggregate_count, 1);
        let request = draft(
            &connection,
            "req_clear",
            Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap(),
            chrono_tz::UTC,
        );
        finish(
            &mut connection,
            &request,
            RequestStatus::Succeeded,
            TokenUsage {
                input_tokens: Some(1),
                output_tokens: Some(1),
                cache_read_tokens: Some(0),
                cache_write_tokens: Some(0),
                total_tokens: Some(2),
            },
        );
        connection.execute_batch("CREATE TRIGGER reject_log_clear BEFORE DELETE ON ai_gateway_request_logs BEGIN SELECT RAISE(ABORT, 'blocked'); END;").unwrap();
        assert!(clear_details(&mut connection).is_err());
        let preserved: i64 = connection
            .query_row("SELECT COUNT(*) FROM ai_gateway_request_logs", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(preserved, 1);
        connection
            .execute_batch("DROP TRIGGER reject_log_clear")
            .unwrap();
        assert_eq!(clear_details(&mut connection).unwrap(), 1);
        run_sqlite_maintenance(&connection).unwrap();
        cleanup(&path);
    }

    #[test]
    fn final_log_and_aggregate_roll_back_together_on_storage_failure() {
        let (path, mut connection) = database("completion-rollback");
        let request = draft(
            &connection,
            "req_rollback",
            Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap(),
            chrono_tz::UTC,
        );
        let started = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
        let item = attempt(
            &request,
            &candidate(),
            1,
            started,
            started + chrono::Duration::seconds(1),
            AttemptStatus::Succeeded,
            None,
            false,
            false,
        )
        .unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER reject_aggregate BEFORE INSERT ON ai_gateway_daily_aggregates BEGIN SELECT RAISE(ABORT, 'blocked'); END;",
            )
            .unwrap();
        let result = complete_request(
            &mut connection,
            &request,
            &[item],
            &RequestCompletion {
                completed_at: (started + chrono::Duration::seconds(1)).to_rfc3339(),
                status: RequestStatus::Succeeded,
                error_code: None,
                usage: TokenUsage {
                    input_tokens: Some(1),
                    output_tokens: Some(1),
                    cache_read_tokens: Some(0),
                    cache_write_tokens: Some(0),
                    total_tokens: Some(2),
                },
            },
        );
        assert!(result.is_err());
        let counts: (i64, i64, i64) = connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM ai_gateway_request_logs), (SELECT COUNT(*) FROM ai_gateway_request_attempts), (SELECT COUNT(*) FROM ai_gateway_daily_aggregates)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(counts, (0, 0, 0));
        cleanup(&path);
    }

    #[test]
    fn concurrent_completions_increment_one_aggregate_without_lost_updates() {
        const REQUESTS: usize = 8;
        let (path, connection) = database("concurrent-completion");
        drop(connection);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(REQUESTS));
        let mut threads = Vec::new();
        for index in 0..REQUESTS {
            let path = path.clone();
            let barrier = std::sync::Arc::clone(&barrier);
            threads.push(std::thread::spawn(move || {
                let mut connection = shared_sqlite::open_at(&path).unwrap();
                let request = draft(
                    &connection,
                    &format!("req_concurrent_{index}"),
                    Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, index as u32)
                        .unwrap(),
                    chrono_tz::UTC,
                );
                barrier.wait();
                finish(
                    &mut connection,
                    &request,
                    RequestStatus::Succeeded,
                    TokenUsage {
                        input_tokens: Some(1),
                        output_tokens: Some(1),
                        cache_read_tokens: Some(0),
                        cache_write_tokens: Some(0),
                        total_tokens: Some(2),
                    },
                );
            }));
        }
        for thread in threads {
            thread.join().unwrap();
        }
        let connection = shared_sqlite::open_at(&path).unwrap();
        let aggregate: (i64, Option<i64>) = connection
            .query_row(
                "SELECT request_count, total_tokens FROM ai_gateway_daily_aggregates",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(aggregate, (REQUESTS as i64, Some((REQUESTS * 2) as i64)));
        cleanup(&path);
    }

    #[test]
    fn aggregate_token_components_remain_unknown_when_any_detail_is_unknown() {
        let (path, mut connection) = database("token-integrity");
        for (request_id, usage) in [
            (
                "req_tokens_known",
                TokenUsage {
                    input_tokens: Some(1),
                    output_tokens: Some(2),
                    cache_read_tokens: Some(3),
                    cache_write_tokens: Some(4),
                    total_tokens: Some(10),
                },
            ),
            (
                "req_tokens_unknown",
                TokenUsage {
                    input_tokens: None,
                    output_tokens: Some(5),
                    cache_read_tokens: None,
                    cache_write_tokens: Some(6),
                    total_tokens: None,
                },
            ),
        ] {
            let request = draft(
                &connection,
                request_id,
                Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap(),
                chrono_tz::UTC,
            );
            finish(&mut connection, &request, RequestStatus::Succeeded, usage);
        }
        let aggregate: (Option<i64>, Option<i64>, Option<i64>, Option<i64>, Option<i64>) =
            connection
                .query_row(
                    "SELECT input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, total_tokens FROM ai_gateway_daily_aggregates",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
                )
                .unwrap();
        assert_eq!(aggregate, (None, Some(7), None, Some(10), None));
        cleanup(&path);
    }

    #[test]
    fn rebuild_preserves_aggregate_only_history_and_refreshes_detail_coverage() {
        let (path, mut connection) = database("aggregate-coverage");
        connection
            .execute(
                "INSERT INTO ai_gateway_daily_aggregates (local_date, timezone_name, account_id_snapshot, account_name_snapshot, group_id_snapshot, group_name_snapshot, public_model_id, request_count, input_tokens, output_tokens, total_tokens, details_covered_through) VALUES ('2020-01-01', 'UTC', 'old-account', 'Old Account', 'default', 'Default', 'gpt-5.6-sol', 99, 9, 9, 18, NULL)",
                [],
            )
            .unwrap();
        let request = draft(
            &connection,
            "req_detail_coverage",
            Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap(),
            chrono_tz::UTC,
        );
        finish(
            &mut connection,
            &request,
            RequestStatus::Succeeded,
            TokenUsage {
                input_tokens: Some(2),
                output_tokens: Some(3),
                cache_read_tokens: Some(0),
                cache_write_tokens: Some(0),
                total_tokens: Some(5),
            },
        );
        connection
            .execute(
                "UPDATE ai_gateway_daily_aggregates SET request_count = 99 WHERE local_date = '2026-08-01'",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO ai_gateway_daily_aggregates (local_date, timezone_name, account_id_snapshot, group_id_snapshot, public_model_id, request_count, details_covered_through) VALUES ('2026-08-02', 'UTC', 'account-log', 'default', 'gpt-5.6-sol', 7, '2026-08-02T00:00:00Z')",
                [],
            )
            .unwrap();
        rebuild_aggregates(
            &mut connection,
            NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 8, 2).unwrap(),
        )
        .unwrap();
        let history: (i64, Option<String>) = connection
            .query_row(
                "SELECT request_count, details_covered_through FROM ai_gateway_daily_aggregates WHERE local_date = '2020-01-01'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(history, (99, None));
        let rebuilt: i64 = connection
            .query_row(
                "SELECT request_count FROM ai_gateway_daily_aggregates WHERE local_date = '2026-08-01'",
                [],
                |row| row.get(0),
        )
        .unwrap();
        assert_eq!(rebuilt, 1);
        let orphan_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_daily_aggregates WHERE local_date = '2026-08-02'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(orphan_count, 0);
        let validation = validate_aggregates(
            &connection,
            NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 8, 2).unwrap(),
        )
        .unwrap();
        assert_eq!(validation.mismatched_rows, 0);
        cleanup(&path);
    }

    #[test]
    fn response_usage_and_persisted_rows_never_include_sensitive_fixture_material() {
        let usage = usage_from_response(
            &serde_json::json!({"usage":{"prompt_tokens":7,"completion_tokens":3,"prompt_tokens_details":{"cached_tokens":2}}}),
        );
        assert_eq!(usage.input_tokens, Some(7));
        assert_eq!(usage.output_tokens, Some(3));
        assert_eq!(usage.cache_read_tokens, Some(2));
        assert_eq!(usage.total_tokens, Some(10));
        let (path, mut connection) = database("redaction");
        let request = draft(
            &connection,
            "req_redacted",
            Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap(),
            chrono_tz::UTC,
        );
        finish(&mut connection, &request, RequestStatus::Succeeded, usage);
        let log_dump: String = connection.query_row(
            "SELECT id || request_id || started_at || COALESCE(completed_at, '') || local_date || timezone_name || endpoint || public_model_id || COALESCE(upstream_model_id_snapshot, '') || COALESCE(api_key_id, '') || COALESCE(api_key_name_snapshot, '') || COALESCE(account_id, '') || COALESCE(account_name_snapshot, '') || COALESCE(group_id_snapshot, '') || COALESCE(group_name_snapshot, '') || status || COALESCE(error_code, '') || COALESCE(price_snapshot_json, '') || COALESCE(estimated_cost_usd, '') FROM ai_gateway_request_logs LIMIT 1",
            [],
            |row| row.get(0),
        ).unwrap();
        let attempt_dump: String = connection.query_row(
            "SELECT id || request_log_id || COALESCE(account_id, '') || account_name_snapshot || COALESCE(group_id_snapshot, '') || COALESCE(group_name_snapshot, '') || COALESCE(upstream_model_id_snapshot, '') || started_at || COALESCE(completed_at, '') || status || COALESCE(error_code, '') FROM ai_gateway_request_attempts LIMIT 1",
            [],
            |row| row.get(0),
        ).unwrap();
        let aggregate_dump: String = connection.query_row(
            "SELECT local_date || timezone_name || account_id_snapshot || COALESCE(account_name_snapshot, '') || group_id_snapshot || COALESCE(group_name_snapshot, '') || public_model_id || COALESCE(estimated_cost_usd, '') FROM ai_gateway_daily_aggregates LIMIT 1",
            [],
            |row| row.get(0),
        ).unwrap();
        let dump = format!("{log_dump}{attempt_dump}{aggregate_dump}");
        for secret in [
            "Authorization",
            "Bearer SAFE_FIXTURE_BEARER",
            "SAFE_FIXTURE_OAUTH_TOKEN",
            "SAFE_FIXTURE_PROMPT_BODY",
            "x-api-key",
        ] {
            assert!(!dump.contains(secret));
        }
        cleanup(&path);
    }
}
