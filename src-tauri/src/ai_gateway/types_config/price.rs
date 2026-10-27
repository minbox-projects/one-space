use serde::{Deserialize, Serialize};

pub(super) fn is_zero_f64(val: &f64) -> bool {
    *val == 0.0
}

/// Normalize an off-peak weekday set: drop values above `6`, deduplicate and
/// sort ascending; an empty result collapses to `None` (meaning every day).
fn normalize_days(days: Option<Vec<u8>>) -> Option<Vec<u8>> {
    let mut days: Vec<u8> = days
        .unwrap_or_default()
        .into_iter()
        .filter(|day| *day <= 6)
        .collect();
    days.sort_unstable();
    days.dedup();
    if days.is_empty() {
        None
    } else {
        Some(days)
    }
}

fn days_are_absent(days: &Option<Vec<u8>>) -> bool {
    normalize_days(days.clone()).is_none()
}

fn serialize_days<S>(days: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    normalize_days(days.clone())
        .unwrap_or_default()
        .serialize(serializer)
}

fn deserialize_days<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<Vec<i64>>::deserialize(deserializer)?;
    let days = raw.map(|values| {
        values
            .into_iter()
            .filter_map(|value| {
                if (0..=6).contains(&value) {
                    Some(value as u8)
                } else {
                    None
                }
            })
            .collect::<Vec<u8>>()
    });
    Ok(normalize_days(days))
}

/// Pricing tier configuration during off-peak hours.
///
/// `days` optionally scopes the window to UTC+8 weekdays (`0` Sunday .. `6`
/// Saturday). Absent, empty or all-invalid means every day; the set is
/// normalized (deduplicated, sorted ascending, out-of-range dropped) on both
/// read and write so an old configuration without the field keeps its behavior.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OffPeakPrice {
    pub start_time: String,
    pub end_time: String,
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub cache_read: f64,
    #[serde(default)]
    pub cache_write: f64,
    #[serde(default)]
    pub output: f64,
    #[serde(
        default,
        skip_serializing_if = "days_are_absent",
        serialize_with = "serialize_days",
        deserialize_with = "deserialize_days"
    )]
    pub days: Option<Vec<u8>>,
}

/// Unit prices for one upstream model, in US dollars per million tokens.
///
/// Prices are matched against the upstream model name recorded at request time;
/// a missing row means the request is unpriced. Editing prices never rewrites
/// already-persisted usage records.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelPrice {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    pub upstream_model: String,
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub cache_read: f64,
    #[serde(default)]
    pub cache_write: f64,
    #[serde(default)]
    pub output: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub off_peaks: Vec<OffPeakPrice>,
}

impl Default for ModelPrice {
    fn default() -> Self {
        Self {
            provider_id: None,
            upstream_model: String::new(),
            input: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
            output: 0.0,
            off_peaks: Vec::new(),
        }
    }
}
