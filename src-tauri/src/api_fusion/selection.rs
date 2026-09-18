use super::{FusionUpstreamProvider, UpstreamProtocol, FAILURE_THRESHOLD};
use rand::seq::SliceRandom;
use std::time::Duration;

/// Outcome class for an upstream attempt, driving switching and auto-disable decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::api_fusion) enum FailureClass {
    /// Auth failures (401/403) disable the provider immediately and switch.
    DisableImmediately,
    /// Counts toward consecutive failures; switches and auto-disables at the threshold.
    Retryable,
    /// Switches without counting as a failure (429/404).
    Transient,
    /// Returns the upstream error to the caller without switching or disabling (400/422/other 4xx).
    ReturnToClient,
}

/// Outcome of resolving a request model against one provider for an inbound protocol.
#[derive(Debug)]
pub(in crate::api_fusion) enum ModelResolution {
    /// Forward using this upstream model.
    Serve(String),
    /// A valid mapping row matches the requested model but declares another
    /// protocol. The provider must not serve this request and must never fall
    /// back to `default_model`; the payload carries the protocol it targets so
    /// the error can name the required endpoint.
    ProtocolMismatch(UpstreamProtocol),
    /// Neither a matching row nor an eligible default model.
    NoMatch,
}

/// Resolve the upstream model for a provider under the inbound `protocol`.
///
/// Rows with an empty `upstream_model` are discarded and count as no match.
/// Disabled rows never serve and never cause a protocol mismatch, but a request
/// that matches only disabled rows is a `NoMatch` and cannot fall back to the
/// default model. An enabled matching row is served with its own remote model
/// only when the row's effective protocol (its own declaration, else the
/// provider protocol) equals `protocol`; when enabled matching rows exist but
/// none matches, the request is a `ProtocolMismatch` and the default model is
/// not used as a fallback. The default model serves an unmapped model only when
/// the provider protocol itself matches.
pub(in crate::api_fusion) fn resolve_model_for_protocol(
    provider: &FusionUpstreamProvider,
    requested: Option<&str>,
    protocol: UpstreamProtocol,
) -> ModelResolution {
    let requested = requested.map(str::trim).filter(|value| !value.is_empty());
    let mut configured: Option<UpstreamProtocol> = None;
    let mut disabled_match = false;
    if let Some(requested) = requested {
        for mapping in provider.mappings.iter().filter(|mapping| {
            mapping.local_model.trim() == requested && !mapping.upstream_model.trim().is_empty()
        }) {
            // A disabled row never serves and never produces a protocol
            // mismatch; it only records that the requested model is mapped but
            // switched off, which later blocks the default-model fallback.
            if !mapping.enabled {
                disabled_match = true;
                continue;
            }
            let effective = mapping.effective_protocol(provider.protocol);
            if effective == protocol {
                return ModelResolution::Serve(mapping.upstream_model.trim().to_string());
            }
            configured.get_or_insert(effective);
        }
    }
    if let Some(configured) = configured {
        return ModelResolution::ProtocolMismatch(configured);
    }
    if disabled_match {
        return ModelResolution::NoMatch;
    }
    provider
        .default_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|_| provider.protocol == protocol)
        .map(|value| ModelResolution::Serve(value.to_string()))
        .unwrap_or(ModelResolution::NoMatch)
}

/// Candidate set: enabled, not auto-disabled, and able to serve the request
/// model under the inbound protocol. A provider is not filtered by its own
/// protocol alone, because a mapping row may declare the inbound protocol.
pub(in crate::api_fusion) fn candidate_providers<'a>(
    providers: &'a [FusionUpstreamProvider],
    requested: Option<&str>,
    protocol: UpstreamProtocol,
) -> Vec<&'a FusionUpstreamProvider> {
    providers
        .iter()
        .filter(|provider| {
            provider.enabled
                && !provider.auto_disabled
                && matches!(
                    resolve_model_for_protocol(provider, requested, protocol),
                    ModelResolution::Serve(_)
                )
        })
        .collect()
}

/// Uniformly shuffle a copy of the candidate list for one request attempt pass.
pub(in crate::api_fusion) fn shuffled_candidates(
    candidates: &[FusionUpstreamProvider],
) -> Vec<FusionUpstreamProvider> {
    let mut ordered = candidates.to_vec();
    ordered.shuffle(&mut rand::thread_rng());
    ordered
}

/// Choose a single candidate uniformly at random (used by coverage-sensitive tests).
#[cfg(test)]
pub(in crate::api_fusion) fn pick_candidate(
    candidates: &[FusionUpstreamProvider],
) -> Option<FusionUpstreamProvider> {
    shuffled_candidates(candidates).into_iter().next()
}

/// Classify an upstream failure.
///
/// Status semantics take priority over the response body shape, so an HTML 401
/// still disables immediately and a 5xx is retryable even without JSON. The
/// caller treats a parsed `< 400` response as success before classifying, so a
/// non-JSON success body falls through to retryable; network errors are always
/// retryable.
pub(in crate::api_fusion) fn classify_failure(
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
        // 429 rate limits are retried by the scheduler but never count toward
        // provider health; 404 and 429 share the "no health count" class.
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
pub(in crate::api_fusion) const MAX_RETRIES_PER_PROVIDER: u32 = 5;

const RETRY_BASE_DELAY_MILLIS: u128 = 2_000;
const RETRY_MAX_DELAY_MILLIS: u128 = 30_000;
const RETRY_JITTER_RATIO: f64 = 0.25;

/// Default delay before the `retry`-th retry (1-based): the spec's
/// `min(2000ms * 2^(retry-1) * (1 + random[0,1]*0.25), 30000ms)`. Header-driven
/// overrides take priority over this.
pub(in crate::api_fusion) fn default_retry_delay(retry: u32) -> Duration {
    let exponent = retry.saturating_sub(1).min(20);
    let base = RETRY_BASE_DELAY_MILLIS.saturating_mul(1u128 << exponent);
    let jitter = 1.0 + rand::random::<f64>() * RETRY_JITTER_RATIO;
    let millis = (base as f64 * jitter).min(RETRY_MAX_DELAY_MILLIS as f64) as u64;
    Duration::from_millis(millis)
}

/// Read the first value of each header, falling through invalid values.
pub(in crate::api_fusion) fn retry_header_delay(
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

/// Whether a classified failure is worth another bounded attempt. `404` is a
/// permanent per-request skip and `ReturnToClient`/`DisableImmediately` never
/// retry; `429` (classified `Transient` so it skips health) is still retried.
pub(in crate::api_fusion) fn is_retryable_failure(class: FailureClass, status: u16) -> bool {
    match class {
        FailureClass::Retryable => true,
        FailureClass::Transient => status == 429,
        FailureClass::DisableImmediately | FailureClass::ReturnToClient => false,
    }
}

/// Record a failure on a provider. Returns `true` when the provider is (or becomes)
/// auto-disabled. `Transient` and `ReturnToClient` never count as failures.
pub(in crate::api_fusion) fn register_failure(
    provider: &mut FusionUpstreamProvider,
    class: FailureClass,
    reason: &str,
    at: u64,
) -> bool {
    let counts = matches!(
        class,
        FailureClass::Retryable | FailureClass::DisableImmediately
    );
    if !counts {
        return false;
    }
    provider.consecutive_failures = provider.consecutive_failures.saturating_add(1);
    provider.last_error_at = Some(at);
    let should_disable = class == FailureClass::DisableImmediately
        || provider.consecutive_failures >= FAILURE_THRESHOLD;
    if should_disable && !provider.auto_disabled {
        provider.auto_disabled = true;
        provider.disabled_reason = Some(reason.to_string());
        provider.disabled_at = Some(at);
    }
    should_disable
}

/// A successful attempt resets the consecutive failure counter.
pub(in crate::api_fusion) fn register_success(provider: &mut FusionUpstreamProvider) {
    provider.consecutive_failures = 0;
    provider.last_error_at = None;
}

/// Manual re-enable clears only the auto-disabled runtime state; user intent is untouched.
pub(in crate::api_fusion) fn manual_reenable(provider: &mut FusionUpstreamProvider) {
    provider.auto_disabled = false;
    provider.disabled_reason = None;
    provider.disabled_at = None;
    provider.consecutive_failures = 0;
    provider.last_error_at = None;
}

/// User toggle touches only the `enabled` intent flag.
pub(in crate::api_fusion) fn set_user_enabled(
    provider: &mut FusionUpstreamProvider,
    enabled: bool,
) {
    provider.enabled = enabled;
}
