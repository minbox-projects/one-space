use std::time::Duration;

/// Outcome class for an upstream attempt, driving switching and auto-disable decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ai_gateway) enum FailureClass {
    /// Auth failures (401/403) disable the provider immediately and switch.
    DisableImmediately,
    /// Counts toward consecutive failures; switches and auto-disables at the threshold.
    /// Quota-exhausted 429s are classified here (via
    /// [`classify_failure_with_message`); plain rate-limit 429s stay [`FailureClass::Transient`].
    Retryable,
    /// Switches without counting as a failure (404 / rate-limit 429).
    Transient,
    /// Returns the upstream error to the caller without switching or disabling (400/422/other 4xx).
    ReturnToClient,
}

/// Classify an upstream failure.
///
/// Status semantics take priority over the response body shape, so an HTML 401
/// still disables immediately and a 5xx is retryable even without JSON. The
/// caller treats a parsed `< 400` response as success before classifying, so a
/// non-JSON success body falls through to retryable; network errors are always
/// retryable.
pub(in crate::ai_gateway) fn classify_failure(
    status: u16,
    network_error: bool,
    _body_parsed: bool,
) -> FailureClass {
    if network_error {
        return FailureClass::Retryable;
    }
    match status {
        401 | 403 => FailureClass::DisableImmediately,
        404 => FailureClass::Transient,
        // A bare 429 is a transient rate limit: retried by the scheduler but
        // never counts toward provider health. Quota-exhausted 429s are
        // upgraded to Retryable by classify_failure_with_message.
        429 => FailureClass::Transient,
        408 => FailureClass::Retryable,
        // 413/422 and every other client error are returned unchanged.
        400..=499 => FailureClass::ReturnToClient,
        500..=599 => FailureClass::Retryable,
        // A 2xx/3xx that did not parse as JSON is unusable and worth another
        // bounded attempt; a parsed 2xx is handled as success before this.
        _ => FailureClass::Retryable,
    }
}

/// Maximum bounded retries after a provider's first attempt in one request
/// (so a provider is contacted at most `1 + MAX_RETRIES_PER_PROVIDER` times).
#[cfg(test)]
pub(in crate::ai_gateway) const MAX_RETRIES_PER_PROVIDER: u32 = 5;

const RETRY_BASE_DELAY_MILLIS: u128 = 2_000;
const RETRY_MAX_DELAY_MILLIS: u128 = 30_000;
const RETRY_JITTER_RATIO: f64 = 0.25;

/// Default delay before the `retry`-th retry (1-based): the spec's
/// `min(2000ms * 2^(retry-1) * (1 + random[0,1]*0.25), 30000ms)`. Header-driven
/// overrides take priority over this.
pub(in crate::ai_gateway) fn default_retry_delay(retry: u32) -> Duration {
    default_retry_delay_with_base(retry, RETRY_BASE_DELAY_MILLIS as u32)
}

pub(in crate::ai_gateway) fn default_retry_delay_with_base(retry: u32, base_ms: u32) -> Duration {
    let exponent = retry.saturating_sub(1).min(20);
    let base = (base_ms as u128).saturating_mul(1u128 << exponent);
    let jitter = 1.0 + rand::random::<f64>() * RETRY_JITTER_RATIO;
    let millis = (base as f64 * jitter).min(RETRY_MAX_DELAY_MILLIS as f64) as u64;
    Duration::from_millis(millis)
}

/// Read the first value of each header, falling through invalid values.
pub(in crate::ai_gateway) fn retry_header_delay(
    headers: &reqwest::header::HeaderMap,
) -> Option<Duration> {
    fn numeric_delay(value: &str, divisor: f64) -> Option<Duration> {
        let number = value.trim().parse::<f64>().ok()?;
        if !number.is_finite() || number < 0.0 {
            return None;
        }
        // Finite but unrepresentably large delays remain valid header values;
        // the request's wait budget will reject them instead of using backoff.
        Some(Duration::try_from_secs_f64(number / divisor).unwrap_or(Duration::MAX))
    }

    if let Some(delay) = headers
        .get("retry-after-ms")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| numeric_delay(value, 1000.0))
    {
        return Some(delay);
    }
    let value = headers.get("retry-after")?.to_str().ok()?.trim();
    if let Some(delay) = numeric_delay(value, 1.0) {
        return Some(delay);
    }
    let date = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    let delay = date.signed_duration_since(chrono::Utc::now());
    if delay <= chrono::Duration::zero() {
        return None;
    }
    delay.to_std().ok()
}

/// Whether an upstream 429 error message describes quota exhaustion rather than
/// a transient rate limit.
///
/// Quota signals (case-insensitive): `quota`, `billing`, `insufficient`,
/// `usage limit`, weekly/monthly period limits, or an `upgrade plan` prompt.
/// A bare `limit`/`exceeded` (e.g. `Rate limit exceeded`) is NOT quota: it
/// stays a transient rate limit so ordinary throttling never disables a
/// mapping row.
pub(in crate::ai_gateway) fn is_quota_exceeded_message(message: Option<&str>) -> bool {
    let Some(message) = message.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    let lower = message.to_ascii_lowercase();
    if lower.contains("quota")
        || lower.contains("billing")
        || lower.contains("insufficient")
        || lower.contains("usage limit")
        || lower.contains("usage_limit")
        || lower.contains("weekly")
        || lower.contains("monthly")
        || lower.contains("upgrade your plan")
        || lower.contains("upgrade plan")
    {
        return true;
    }
    if lower.contains("balance")
        && (lower.contains("exceed")
            || lower.contains("insufficient")
            || lower.contains("limit")
            || lower.contains("deplet")
            || lower.contains("empty")
            || lower.contains("zero"))
    {
        return true;
    }
    if lower.contains("plan")
        && (lower.contains("limit")
            || lower.contains("usage")
            || lower.contains("reset")
            || lower.contains("quota")
            || lower.contains("upgrade"))
    {
        return true;
    }
    false
}

/// Whether an upstream error message explicitly names a credential/authentication
/// problem, case-insensitively.
///
/// Credential signals: `invalid api key`, `api key not valid`, `incorrect api
/// key`, `invalid key`, `unauthorized`, `unauthenticated`, `authentication`,
/// `auth failed`, `invalid token`, `token expired`, `invalid credentials` or
/// `invalid authorization`. Absent or whitespace-only text is never a credential
/// signal, so an empty 403 body stays a non-credential failure.
pub(in crate::ai_gateway) fn is_authentication_error_message(message: Option<&str>) -> bool {
    let Some(message) = message.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    let lower = message.to_ascii_lowercase();
    const SIGNALS: [&str; 12] = [
        "invalid api key",
        "api key not valid",
        "incorrect api key",
        "invalid key",
        "unauthorized",
        "unauthenticated",
        "authentication",
        "auth failed",
        "invalid token",
        "token expired",
        "invalid credentials",
        "invalid authorization",
    ];
    SIGNALS.iter().any(|signal| lower.contains(signal))
}

/// Classify an upstream failure with the sanitized error text available.
///
/// A quota-message classifier hit on HTTP 400, 402 or 429 is `Retryable`;
/// any other 429 stays `Transient` and any other 400/402 keeps its
/// `ReturnToClient` class. Such a matching quota message always yields a
/// key-scoped mark in `runtime_http::key_failure_kind`, so no other path can
/// observe the upgraded class as a plain retry and mapping health is not
/// registered for it. Everything else delegates to [`classify_failure`].
pub(in crate::ai_gateway) fn classify_failure_with_message(
    status: u16,
    network_error: bool,
    body_parsed: bool,
    error_message: Option<&str>,
) -> FailureClass {
    if matches!(status, 400 | 402 | 429)
        && !network_error
        && is_quota_exceeded_message(error_message)
    {
        return FailureClass::Retryable;
    }
    classify_failure(status, network_error, body_parsed)
}

/// Whether a classified failure is worth another bounded attempt on the same
/// provider. `404` is a permanent per-request skip and
/// `ReturnToClient`/`DisableImmediately` never retry; a plain rate-limit 429
/// (classified `Transient`) is still retried. A quota-exhausted 429 is
/// classified `Retryable` for health counting but must NOT be retried on the
/// same provider (its quota will not recover inside the request budget), so
/// callers pass the same error text here to suppress the same-provider retry
/// while keeping the health count.
pub(in crate::ai_gateway) fn is_retryable_failure(class: FailureClass, status: u16) -> bool {
    match class {
        FailureClass::Retryable => true,
        FailureClass::Transient => status == 429,
        FailureClass::DisableImmediately | FailureClass::ReturnToClient => false,
    }
}

/// Quota-aware variant of [`is_retryable_failure`]: quota-exhausted 429s count
/// toward health (via their `Retryable` class) but never requeue the same
/// provider.
pub(in crate::ai_gateway) fn is_retryable_with_message(
    class: FailureClass,
    status: u16,
    error_message: Option<&str>,
) -> bool {
    if status == 429 && is_quota_exceeded_message(error_message) {
        return false;
    }
    is_retryable_failure(class, status)
}
