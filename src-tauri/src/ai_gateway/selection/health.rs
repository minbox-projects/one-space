use super::failures::FailureClass;
use super::probe::MappingTarget;
use super::super::{GatewayUpstreamProvider, ModelMapping};
#[cfg(test)]
use super::super::FAILURE_THRESHOLD;

/// Re-arm a failed probe's cooldown: every row matching `target`'s trimmed key
/// stays auto-disabled, keeps its consecutive-failure counter, and moves
/// `disabled_at` to `at`. With `update_details` the failure also stamps
/// `last_error_at` and `disabled_reason`; a suppressed transport failure passes
/// `false` so only the cooldown moves.
pub(in crate::ai_gateway) fn rearm_mapping_probe_cooldown(
    provider: &mut GatewayUpstreamProvider,
    target: &MappingTarget,
    reason: &str,
    at: u64,
    update_details: bool,
) {
    for mapping in provider.mappings.iter_mut().filter(|mapping| {
        mapping_matches_key(mapping, &target.local_model, &target.upstream_model)
    }) {
        mapping.auto_disabled = true;
        mapping.disabled_at = Some(at);
        if update_details {
            mapping.last_error_at = Some(at);
            mapping.disabled_reason = Some(reason.to_string());
        }
    }
}

/// Record a failure on every row matching `target`'s trimmed key. Returns `true`
/// when that key is (or becomes) auto-disabled. `Transient` and `ReturnToClient`
/// never count as failures.
#[cfg(test)]
pub(in crate::ai_gateway) fn register_mapping_failure(
    provider: &mut GatewayUpstreamProvider,
    target: &MappingTarget,
    class: FailureClass,
    reason: &str,
    at: u64,
) -> bool {
    register_mapping_failure_with_threshold(provider, target, class, reason, at, FAILURE_THRESHOLD)
}

pub(in crate::ai_gateway) fn register_mapping_failure_with_threshold(
    provider: &mut GatewayUpstreamProvider,
    target: &MappingTarget,
    class: FailureClass,
    reason: &str,
    at: u64,
    failure_threshold: u32,
) -> bool {
    let counts = matches!(
        class,
        FailureClass::Retryable | FailureClass::DisableImmediately
    );
    if !counts {
        return false;
    }
    let mut disabled = false;
    for mapping in provider.mappings.iter_mut().filter(|mapping| {
        mapping_matches_key(mapping, &target.local_model, &target.upstream_model)
    }) {
        // Frozen Step 2 rule: an immediate auth disable records reason/at/
        // last_error_at but must not advance the transient-failure counter, so
        // the counter keeps meaning consecutive Retryable failures and a 401/403
        // row can never reach the probe threshold through the counter.
        if class == FailureClass::Retryable {
            mapping.consecutive_failures = mapping.consecutive_failures.saturating_add(1);
        }
        mapping.last_error_at = Some(at);
        let should_disable = class == FailureClass::DisableImmediately
            || (failure_threshold > 0 && mapping.consecutive_failures >= failure_threshold);
        if should_disable && !mapping.auto_disabled {
            mapping.auto_disabled = true;
            mapping.disabled_reason = Some(reason.to_string());
            mapping.disabled_at = Some(at);
        }
        disabled |= mapping.auto_disabled;
    }
    disabled
}

/// A successful attempt resets the consecutive failure counter and last-error
/// value of every row matching `target`'s trimmed key.
pub(in crate::ai_gateway) fn register_mapping_success(
    provider: &mut GatewayUpstreamProvider,
    target: &MappingTarget,
) {
    for mapping in provider.mappings.iter_mut().filter(|mapping| {
        mapping_matches_key(mapping, &target.local_model, &target.upstream_model)
    }) {
        mapping.consecutive_failures = 0;
        mapping.last_error_at = None;
    }
}

/// Clear a row's runtime health state only; the user's `enabled` intent is untouched.
pub(in crate::ai_gateway) fn clear_mapping_runtime_state(mapping: &mut ModelMapping) {
    mapping.auto_disabled = false;
    mapping.disabled_reason = None;
    mapping.disabled_at = None;
    mapping.consecutive_failures = 0;
    mapping.last_error_at = None;
}

/// Whether a row's trimmed `(local_model, upstream_model)` equals the given key.
pub(in crate::ai_gateway) fn mapping_matches_key(
    mapping: &ModelMapping,
    local_model: &str,
    upstream_model: &str,
) -> bool {
    mapping.local_model.trim() == local_model.trim()
        && mapping.upstream_model.trim() == upstream_model.trim()
}

/// Manual re-enable clears the runtime state of every auto-disabled row; the
/// user's `enabled` intent is untouched and a row that is not auto-disabled
/// keeps its counter.
pub(in crate::ai_gateway) fn manual_reenable(provider: &mut GatewayUpstreamProvider) {
    for mapping in provider.mappings.iter_mut() {
        if mapping.auto_disabled {
            clear_mapping_runtime_state(mapping);
        }
    }
}

/// User toggle touches only the `enabled` intent flag.
pub(in crate::ai_gateway) fn set_user_enabled(
    provider: &mut GatewayUpstreamProvider,
    enabled: bool,
) {
    provider.enabled = enabled;
}
