//! Cost and off-peak pricing calculations for recorded token usage.

use super::records::{UsageCostBreakdown, UsageTokens};
use super::range::utc8_offset;
use super::super::ModelPrice;
use chrono::{Datelike, Timelike};

/// Cost in US dollars for the four token tiers, prices are per million tokens.
///
/// Test-only reference arithmetic; production billing goes through
/// [`compute_cost_at_time_with_breakdown`].
#[cfg(test)]
pub fn compute_cost(price: &ModelPrice, tokens: &UsageTokens) -> f64 {
    (price.input * tokens.input_tokens as f64
        + price.cache_read * tokens.cache_read_tokens as f64
        + price.cache_write * tokens.cache_write_tokens as f64
        + price.output * tokens.output_tokens as f64)
        / 1_000_000.0
}

/// Check whether a timestamp_ms falls into the off-peak time window [start_time, end_time) in UTC+8.
///
/// `start_time` and `end_time` are strings in "HH:mm" 24-hour format (e.g. "00:30", "08:30").
/// - When start_time < end_time: [start_time, end_time) within the same UTC+8 day.
/// - When start_time > end_time: overnight window, [start_time, 24:00) or [00:00, end_time).
/// - When start_time == end_time: zero duration window (evaluates to false).
///
/// Test-only weekday-less compatibility entry point; production pricing goes
/// through [`is_off_peak_with_days`].
#[cfg(test)]
pub fn is_off_peak(timestamp_ms: i64, start_time: &str, end_time: &str) -> bool {
    is_off_peak_with_days(timestamp_ms, start_time, end_time, None)
}

/// Days-aware variant of [`is_off_peak`]. `days` is the optional UTC+8 weekday
/// set (`0` Sunday .. `6` Saturday) the window applies to; `None`, an empty set
/// or a set with no value in `0..=6` means every day.
fn is_off_peak_with_days(
    timestamp_ms: i64,
    start_time: &str,
    end_time: &str,
    days: Option<&[u8]>,
) -> bool {
    let dt = match chrono::DateTime::from_timestamp_millis(timestamp_ms) {
        Some(dt) => dt.with_timezone(&utc8_offset()),
        None => return false,
    };
    if !matches_off_peak_days(days, dt.weekday().num_days_from_sunday()) {
        return false;
    }
    let current_minute = dt.hour() * 60 + dt.minute();

    let parse_minute = |s: &str| -> Option<u32> {
        let mut parts = s.trim().split(':');
        let h: u32 = parts.next()?.parse().ok()?;
        let m: u32 = parts.next()?.parse().ok()?;
        if h < 24 && m < 60 {
            Some(h * 60 + m)
        } else {
            None
        }
    };

    let (start_min, end_min) = match (parse_minute(start_time), parse_minute(end_time)) {
        (Some(s), Some(e)) => (s, e),
        _ => return false,
    };

    if start_min == end_min {
        false
    } else if start_min < end_min {
        current_minute >= start_min && current_minute < end_min
    } else {
        current_minute >= start_min || current_minute < end_min
    }
}

/// Whether the request's UTC+8 weekday is included by an optional weekday set.
/// Out-of-range values are ignored; an empty/all-invalid set means every day.
fn matches_off_peak_days(days: Option<&[u8]>, weekday: u32) -> bool {
    let Some(days) = days else {
        return true;
    };
    let mut has_valid = false;
    for day in days {
        if *day > 6 {
            continue;
        }
        has_valid = true;
        if *day as u32 == weekday {
            return true;
        }
    }
    !has_valid
}

/// Cost in US dollars for the four token tiers.
///
/// If off-peak pricing configurations are present and `timestamp_ms` falls within an off-peak
/// window in UTC+8, the first matching off-peak pricing tier is used; otherwise, the standard pricing tier is used.
///
/// Test-only wrapper; production billing goes through
/// [`compute_cost_at_time_with_breakdown`].
#[cfg(test)]
pub fn compute_cost_at_time(
    price: &ModelPrice,
    tokens: &UsageTokens,
    timestamp_ms: i64,
) -> f64 {
    compute_cost_at_time_with_breakdown(price, tokens, timestamp_ms).0
}

/// Cost and frozen pricing details for the effective UTC+8 tier.
pub fn compute_cost_at_time_with_breakdown(
    price: &ModelPrice,
    tokens: &UsageTokens,
    timestamp_ms: i64,
) -> (f64, UsageCostBreakdown) {
    let window = price.off_peaks.iter().find(|window| {
        is_off_peak_with_days(
            timestamp_ms,
            &window.start_time,
            &window.end_time,
            window.days.as_deref(),
        )
    });
    let (input_price, cache_read_price, cache_write_price, output_price) = match window {
        Some(window) => (window.input, window.cache_read, window.cache_write, window.output),
        None => (price.input, price.cache_read, price.cache_write, price.output),
    };
    let input = input_price * tokens.input_tokens as f64;
    let cache_read = cache_read_price * tokens.cache_read_tokens as f64;
    let cache_write = cache_write_price * tokens.cache_write_tokens as f64;
    let output = output_price * tokens.output_tokens as f64;
    // Keep the existing total's sum-before-division arithmetic and tier order.
    let total = (input + cache_read + cache_write + output) / 1_000_000.0;
    (
        total,
        UsageCostBreakdown {
            input_price,
            output_price,
            cache_read_price,
            cache_write_price,
            input_cost: input / 1_000_000.0,
            output_cost: output / 1_000_000.0,
            cache_read_cost: cache_read / 1_000_000.0,
            cache_write_cost: cache_write / 1_000_000.0,
        },
    )
}

/// Match a price row scoped to the forwarded provider and upstream model.
///
/// Matching is exact and case-sensitive; a row that belongs to another provider
/// or carries no provider id never prices a request.
pub fn match_price_for_provider<'a>(
    provider_id: &str,
    upstream_model: &str,
    prices: &'a [ModelPrice],
) -> Option<&'a ModelPrice> {
    prices.iter().find(|price| {
        price.provider_id.as_deref() == Some(provider_id)
            && price.upstream_model == upstream_model
    })
}
