use super::{FusionUpstreamProvider, FAILURE_THRESHOLD};
use rand::seq::SliceRandom;

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

/// Resolve the upstream model for a provider: exact mapping first, default model fallback.
/// Returns `None` when the provider can neither map nor fall back for the request.
pub(in crate::api_fusion) fn resolve_model(
    provider: &FusionUpstreamProvider,
    requested: Option<&str>,
) -> Option<String> {
    if let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) {
        if let Some(mapping) = provider
            .mappings
            .iter()
            .find(|mapping| mapping.local_model.trim() == requested)
        {
            let upstream = mapping.upstream_model.trim();
            if !upstream.is_empty() {
                return Some(upstream.to_string());
            }
        }
    }
    provider
        .default_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

pub(in crate::api_fusion) fn can_serve(
    provider: &FusionUpstreamProvider,
    requested: Option<&str>,
) -> bool {
    resolve_model(provider, requested).is_some()
}

/// Candidate set: enabled, not auto-disabled, and able to resolve the request model.
pub(in crate::api_fusion) fn candidate_providers<'a>(
    providers: &'a [FusionUpstreamProvider],
    requested: Option<&str>,
) -> Vec<&'a FusionUpstreamProvider> {
    providers
        .iter()
        .filter(|provider| provider.enabled && !provider.auto_disabled && can_serve(provider, requested))
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
/// Non-JSON / unparsable bodies are treated as retryable regardless of status (including 2xx),
/// and network errors are always retryable.
pub(in crate::api_fusion) fn classify_failure(
    status: u16,
    network_error: bool,
    body_parsed: bool,
) -> FailureClass {
    if network_error {
        return FailureClass::Retryable;
    }
    if !body_parsed {
        return FailureClass::Retryable;
    }
    match status {
        401 | 403 => FailureClass::DisableImmediately,
        429 | 404 => FailureClass::Transient,
        500..=599 => FailureClass::Retryable,
        400 | 422 => FailureClass::ReturnToClient,
        other if (400..500).contains(&other) => FailureClass::ReturnToClient,
        _ => FailureClass::Retryable,
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
