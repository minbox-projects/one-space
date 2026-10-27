//! UTC+8 time-range and bucket resolution for usage queries.

/// Milliseconds in one UTC day.
pub(super) const DAY_MS: i64 = 86_400_000;
/// Milliseconds of the fixed UTC+8 offset (the product's reporting timezone).
const UTC8_OFFSET_MS: i64 = 8 * 3_600_000;

pub(super) fn utc8_offset() -> chrono::FixedOffset {
    chrono::FixedOffset::east_opt((UTC8_OFFSET_MS / 1000) as i32)
        .expect("UTC+8 is a valid fixed offset")
}

/// Half-open millisecond range `[start, end)`; `None` means unbounded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::ai_gateway) struct TimeRange {
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
}

/// UTC+8 midnight (epoch milliseconds) of the natural day containing `now_ms`.
fn utc8_day_start_ms(now_ms: i64) -> i64 {
    (now_ms + UTC8_OFFSET_MS).div_euclid(DAY_MS) * DAY_MS - UTC8_OFFSET_MS
}

/// Resolve a quick-range `days` selector to UTC+8 day boundaries.
///
/// `None` means "all"; `Some(1)` means today; `Some(n)` covers today plus the
/// previous `n - 1` natural days, with the day boundary fixed at UTC+8 midnight.
pub(in crate::ai_gateway) fn resolve_range(days: Option<i64>, now_ms: i64) -> TimeRange {
    let Some(days) = days.filter(|days| *days >= 1) else {
        return TimeRange::default();
    };
    let today_start_ms = utc8_day_start_ms(now_ms);
    TimeRange {
        start_ms: Some(today_start_ms - (days - 1) * DAY_MS),
        end_ms: Some(today_start_ms + DAY_MS),
    }
}

/// A resolved range selector together with the bucket granularity its window
/// implies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ai_gateway) struct ResolvedRange {
    pub range: TimeRange,
    /// `true` when the window spans exactly one UTC+8 natural day, which is the
    /// condition for hourly buckets.
    pub hourly: bool,
}

/// Resolve a range selector to a UTC+8 window and its bucket granularity.
///
/// The selector is trimmed first:
/// - `None`, an empty string or `"all"` is the unbounded all-time window;
/// - `"today"` / `"yesterday"` are one UTC+8 natural day (hourly buckets);
/// - `"7d"` / `"15d"` / `"30d"` reuse [`resolve_range`], so their windows stay
///   byte-identical to the previous `days = 7/15/30` behavior.
///
/// Any other selector is rejected with the supported vocabulary; it never
/// silently falls back to all-time or today.
pub(in crate::ai_gateway) fn resolve_range_selector(
    selector: Option<&str>,
    now_ms: i64,
) -> Result<ResolvedRange, String> {
    let selector = selector.map(str::trim).unwrap_or("");
    let range = match selector {
        "" | "all" => TimeRange::default(),
        "today" => resolve_range(Some(1), now_ms),
        "yesterday" => {
            let today_start_ms = utc8_day_start_ms(now_ms);
            TimeRange {
                start_ms: Some(today_start_ms - DAY_MS),
                end_ms: Some(today_start_ms),
            }
        }
        "7d" => resolve_range(Some(7), now_ms),
        "15d" => resolve_range(Some(15), now_ms),
        "30d" => resolve_range(Some(30), now_ms),
        other => {
            return Err(format!(
                "unsupported range '{other}': expected one of 'today', 'yesterday', '7d', '15d', '30d' or 'all'"
            ))
        }
    };
    let hourly = matches!(
        (range.start_ms, range.end_ms),
        (Some(start), Some(end)) if end - start == DAY_MS
    );
    Ok(ResolvedRange { range, hourly })
}

pub(super) fn day_index_label(day_index: i64) -> String {
    let utc_ms = day_index * DAY_MS - UTC8_OFFSET_MS;
    chrono::DateTime::from_timestamp_millis(utc_ms)
        .map(|instant| {
            instant
                .with_timezone(&utc8_offset())
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_default()
}
