use super::model::UsageWindow;
use chrono::{Duration, Local, NaiveDate, TimeZone};

pub(super) fn usage_window(days: u16) -> UsageWindow {
    let end_date = Local::now().date_naive();
    let start_date = end_date - Duration::days(days.saturating_sub(1) as i64);
    let start_ms = Local
        .from_local_datetime(&start_date.and_hms_opt(0, 0, 0).expect("valid start"))
        .single()
        .or_else(|| {
            Local
                .from_local_datetime(&start_date.and_hms_opt(1, 0, 0).expect("valid start"))
                .single()
        })
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0);
    let next_date = end_date + Duration::days(1);
    let end_ms = Local
        .from_local_datetime(&next_date.and_hms_opt(0, 0, 0).expect("valid end"))
        .single()
        .or_else(|| {
            Local
                .from_local_datetime(&next_date.and_hms_opt(1, 0, 0).expect("valid end"))
                .single()
        })
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(i64::MAX);
    UsageWindow {
        days,
        start_date,
        end_date,
        start_ms,
        end_ms,
    }
}

pub(super) fn usage_day_window(date: NaiveDate) -> UsageWindow {
    let start_ms = Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).expect("valid start"))
        .single()
        .or_else(|| {
            Local
                .from_local_datetime(&date.and_hms_opt(1, 0, 0).expect("valid start"))
                .single()
        })
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0);
    let next_date = date + Duration::days(1);
    let end_ms = Local
        .from_local_datetime(&next_date.and_hms_opt(0, 0, 0).expect("valid end"))
        .single()
        .or_else(|| {
            Local
                .from_local_datetime(&next_date.and_hms_opt(1, 0, 0).expect("valid end"))
                .single()
        })
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(i64::MAX);
    UsageWindow {
        days: 1,
        start_date: date,
        end_date: date,
        start_ms,
        end_ms,
    }
}
