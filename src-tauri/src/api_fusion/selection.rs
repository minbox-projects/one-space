use super::{FusionUpstreamProvider, UpstreamProtocol, FAILURE_THRESHOLD};
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
/// A matching row is served with its own remote model only when the row's
/// effective protocol (its own declaration, else the provider protocol) equals
/// `protocol`; when matching rows exist but none matches, the request is a
/// `ProtocolMismatch` and the default model is not used as a fallback. The
/// default model serves an unmapped model only when the provider protocol
/// itself matches.
pub(in crate::api_fusion) fn resolve_model_for_protocol(
    provider: &FusionUpstreamProvider,
    requested: Option<&str>,
    protocol: UpstreamProtocol,
) -> ModelResolution {
    let requested = requested.map(str::trim).filter(|value| !value.is_empty());
    let mut configured: Option<UpstreamProtocol> = None;
    if let Some(requested) = requested {
        for mapping in provider.mappings.iter().filter(|mapping| {
            mapping.local_model.trim() == requested && !mapping.upstream_model.trim().is_empty()
        }) {
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
