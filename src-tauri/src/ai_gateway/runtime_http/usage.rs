use super::*;

/// Persist the request's buffered attempt rows with exactly one terminal row.
///
/// `terminal_index` names the buffered attempt that is the request's terminal
/// row: the successful, `ReturnToClient`, mid-stream-failure or chronologically
/// last exhausted attempt. The caller only reaches this helper for a completed
/// business outcome with at least one buffered attempt; a downstream-cancelled
/// or undeliverable request discards its whole buffer before logging, and a
/// request without a completed attempt writes the gateway's own synthetic
/// terminal row instead. All rows of one request go through one store and one
/// batch, and any storage failure is logged and swallowed — the response has
/// already been produced and must not be affected.
pub(super) async fn record_request_usage_logs(
    config: &GatewayConfig,
    local_model: Option<&str>,
    reasoning_effort: Option<String>,
    attempts: &[AttemptLog],
    terminal_index: usize,
) {
    let local_model = local_model.unwrap_or_default();
    let entries: Vec<UsageLogEntry> = attempts
        .iter()
        .enumerate()
        .map(|(index, attempt)| UsageLogEntry {
            record: build_usage_log_row(
                config,
                local_model,
                &attempt.provider_id,
                &attempt.provider_name,
                &attempt.upstream_model,
                attempt.result,
                attempt.status,
                attempt.usage,
                attempt.duration_ms,
                attempt.first_token_ms,
                attempt.avg_tps,
                attempt.error_message.clone(),
                index == terminal_index,
                reasoning_effort.clone(),
                attempt.resolved_reasoning_effort.clone(),
                attempt.native_reasoning.clone(),
            ),
            accounting: UsageAccounting {
                present: attempt.usage.is_some(),
                valid: attempt.usage_valid,
            },
        })
        .collect();
    write_usage_log_entries(config, entries).await;
}

/// The gateway's own terminal row, attributed to no provider: an empty upstream
/// model, no usage, and the gateway-generated diagnostic that accompanied the
/// 502 response (for example the all-unavailable or no-candidate explanation).
pub(super) fn synthetic_terminal_row(
    config: &GatewayConfig,
    local_model: &str,
    result: UsageResult,
    status: u16,
    duration_ms: u64,
    error_message: Option<&str>,
    reasoning_effort: Option<String>,
    resolved_reasoning_effort: Option<String>,
) -> UsageLogRecord {
    build_usage_log_row(
        config,
        local_model,
        "",
        "",
        "",
        result,
        status,
        None,
        duration_ms,
        None,
        None,
        error_message.map(str::to_string),
        true,
        reasoning_effort,
        resolved_reasoning_effort,
        None,
    )
}

/// Build one request-log row. The amount is fixed at record time from the price
/// table, so later price edits never rewrite history.
#[allow(clippy::too_many_arguments)]
fn build_usage_log_row(
    config: &GatewayConfig,
    local_model: &str,
    provider_id: &str,
    provider_name: &str,
    upstream_model: &str,
    result: UsageResult,
    status: u16,
    usage: Option<UsageTokens>,
    duration_ms: u64,
    first_token_ms: Option<f64>,
    avg_tps: Option<f64>,
    error_message: Option<String>,
    terminal: bool,
    reasoning_effort: Option<String>,
    resolved_reasoning_effort: Option<String>,
    native_reasoning: Option<NativeReasoningSnapshot>,
) -> UsageLogRecord {
    let timestamp_ms = now_millis();
    let tokens = usage.unwrap_or_default();
    let (amount, cost_breakdown) =
        match match_price_for_provider(provider_id, upstream_model, &config.model_prices) {
            Some(price) => {
                let (amount, breakdown) =
                    compute_cost_at_time_with_breakdown(price, &tokens, timestamp_ms);
                (Some(amount), Some(breakdown))
            }
            None => (None, None),
        };
    UsageLogRecord {
        timestamp_ms,
        local_model: local_model.to_string(),
        upstream_model: upstream_model.to_string(),
        provider_id: provider_id.to_string(),
        provider_name: provider_name.to_string(),
        result,
        status,
        input_tokens: tokens.input_tokens,
        cache_read_tokens: tokens.cache_read_tokens,
        cache_write_tokens: tokens.cache_write_tokens,
        output_tokens: tokens.output_tokens,
        total_tokens: tokens.total(),
        amount,
        cost_breakdown,
        duration_ms,
        first_token_ms,
        avg_tps,
        error_message,
        terminal,
        reasoning_effort,
        resolved_reasoning_effort,
        native_reasoning,
    }
}

/// Write one request row through a single store call on the blocking executor;
/// a storage failure is reported only as a swallowed log-write warning.
/// A synthetic gateway row carries no upstream usage object.
pub(super) async fn record_usage_log(config: &GatewayConfig, record: &UsageLogRecord) {
    let retention = normalize_retention_days(config.usage_retention_days);
    let record = record.clone();
    let accounting = UsageAccounting {
        present: false,
        valid: false,
    };
    let write = tokio::task::spawn_blocking(move || {
        UsageLogStore::default_store()
            .and_then(|store| store.append_with_accounting(&record, accounting, retention))
    })
    .await;
    match write {
        Ok(Err(error)) => log::warn!("AI gateway usage log write failed: {error}"),
        Err(error) => log::warn!("AI gateway usage log task failed: {error}"),
        Ok(Ok(())) => {}
    }
}

/// Write every entry of one request through a single store and batch on the
/// blocking executor; a storage failure is reported only as a swallowed
/// log-write warning.
pub(super) async fn write_usage_log_entries(config: &GatewayConfig, entries: Vec<UsageLogEntry>) {
    let retention = normalize_retention_days(config.usage_retention_days);
    let write = tokio::task::spawn_blocking(move || {
        UsageLogStore::default_store()
            .and_then(|store| store.append_batch_with_accounting(&entries, retention))
    })
    .await;
    match write {
        Ok(Err(error)) => log::warn!("AI gateway usage log write failed: {error}"),
        Err(error) => log::warn!("AI gateway usage log task failed: {error}"),
        Ok(Ok(())) => {}
    }
}
