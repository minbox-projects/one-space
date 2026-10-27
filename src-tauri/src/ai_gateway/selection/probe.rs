use super::candidates::can_convert_from;
use super::super::{
    GatewayUpstreamProvider, UpstreamProtocol, AUTO_DISABLE_PROBE_COOLDOWN_SECS, FAILURE_THRESHOLD,
};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// Identity of one mapping row: the provider it belongs to plus its trimmed
/// `(local_model, upstream_model)` key.
///
/// Runtime health settles per row, so a failure of one row never touches a
/// sibling row or the provider's own legacy state.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::ai_gateway) struct MappingTarget {
    pub(in crate::ai_gateway) provider_id: String,
    pub(in crate::ai_gateway) local_model: String,
    pub(in crate::ai_gateway) upstream_model: String,
}

impl MappingTarget {
    /// Build a target, trimming every component.
    pub(in crate::ai_gateway) fn new(
        provider_id: &str,
        local_model: &str,
        upstream_model: &str,
    ) -> Self {
        Self {
            provider_id: provider_id.trim().to_string(),
            local_model: local_model.trim().to_string(),
            upstream_model: upstream_model.trim().to_string(),
        }
    }

    /// The row a served request settles on when it resolved through a mapping
    /// row rather than the provider's `default_model`.
    ///
    /// Returns `None` unless the trimmed requested model is non-empty, matches a
    /// row's trimmed `local_model` and that row's trimmed `upstream_model` equals
    /// the resolved upstream model. A `default_model` attempt therefore settles
    /// on no row and records no health outcome.
    pub(in crate::ai_gateway) fn for_request(
        provider: &GatewayUpstreamProvider,
        requested: Option<&str>,
        upstream_model: &str,
    ) -> Option<Self> {
        let requested = requested.map(str::trim).filter(|value| !value.is_empty())?;
        let upstream_model = upstream_model.trim();
        provider
            .mappings
            .iter()
            .find(|mapping| {
                mapping.local_model.trim() == requested
                    && mapping.upstream_model.trim() == upstream_model
            })
            .map(|_| Self::new(&provider.id, requested, upstream_model))
    }
}

/// One eligible half-open probe: the provider whose auto-disabled row may be
/// retried, the mapping row identity it settles on, and the upstream model to
/// forward. Returned by [`find_probe_candidate`].
#[derive(Debug, Clone)]
pub(in crate::ai_gateway) struct ProbeCandidate {
    pub(in crate::ai_gateway) provider: GatewayUpstreamProvider,
    pub(in crate::ai_gateway) target: MappingTarget,
    pub(in crate::ai_gateway) upstream_model: String,
    /// The effective protocol of the probed row. The probe attempt forwards
    /// under this protocol even though the auto-disabled row is invisible to
    /// the normal served-row resolution, so a converted probe still reaches the
    /// Anthropic Messages endpoint (REQ-005/AC-010).
    pub(in crate::ai_gateway) protocol: UpstreamProtocol,
}

/// Whether a row's trimmed `disabled_reason` denotes an immediate auth disable
/// (401/403) that must stay manual-only. Rows persisted by older builds could
/// have reached the threshold counter under the previous counting rule, so the
/// reason prefix is the discriminator for them; no threshold disable produces
/// this prefix.
fn is_immediate_auth_disable_reason(reason: Option<&str>) -> bool {
    let reason = reason.unwrap_or("").trim();
    reason.starts_with("HTTP 401") || reason.starts_with("HTTP 403")
}

/// Find at most one eligible half-open probe candidate for a request.
///
/// A row is eligible only when its provider and the row are both enabled, the
/// row is auto-disabled by a transient-failure threshold (`consecutive_failures
/// >= FAILURE_THRESHOLD`) with a `disabled_reason` that is not an immediate auth
/// disable, it has a non-empty trimmed `upstream_model`, its trimmed
/// `local_model` equals the requested model, its effective protocol equals the
/// inbound `protocol`, its `disabled_at` is present, and the cooldown
/// `now - disabled_at >= AUTO_DISABLE_PROBE_COOLDOWN_SECS` has elapsed. Rows
/// whose provider is already serving as a healthy candidate are excluded.
///
/// At most one candidate is returned: the oldest `disabled_at` first, breaking
/// ties by provider id, then `local_model`, then `upstream_model` (trimmed,
/// lexicographic). A missing or blank requested model yields no candidate.
pub(in crate::ai_gateway) fn find_probe_candidate(
    providers: &[GatewayUpstreamProvider],
    requested: Option<&str>,
    protocol: UpstreamProtocol,
    healthy_provider_ids: &[String],
    now: u64,
) -> Option<ProbeCandidate> {
    find_probe_candidate_with_settings(
        providers,
        requested,
        protocol,
        healthy_provider_ids,
        now,
        AUTO_DISABLE_PROBE_COOLDOWN_SECS,
        FAILURE_THRESHOLD,
    )
}

pub(in crate::ai_gateway) fn find_probe_candidate_with_settings(
    providers: &[GatewayUpstreamProvider],
    requested: Option<&str>,
    protocol: UpstreamProtocol,
    healthy_provider_ids: &[String],
    now: u64,
    probe_cooldown_secs: u64,
    failure_threshold: u32,
) -> Option<ProbeCandidate> {
    let requested = requested.map(str::trim).filter(|value| !value.is_empty())?;
    let mut best: Option<((u64, String, String, String), ProbeCandidate)> = None;
    for provider in providers.iter().filter(|provider| provider.enabled) {
        if healthy_provider_ids
            .iter()
            .any(|id| id == &provider.id)
        {
            continue;
        }
        for mapping in provider
            .mappings
            .iter()
            .filter(|mapping| mapping.enabled && mapping.auto_disabled)
        {
            let Some(disabled_at) = mapping.disabled_at else {
                continue;
            };
            if now.saturating_sub(disabled_at) < probe_cooldown_secs {
                continue;
            }
            if failure_threshold > 0 && mapping.consecutive_failures < failure_threshold {
                continue;
            }
            if is_immediate_auth_disable_reason(mapping.disabled_reason.as_deref()) {
                continue;
            }
            if mapping.local_model.trim() != requested {
                continue;
            }
            let upstream_model = mapping.upstream_model.trim().to_string();
            if upstream_model.is_empty() {
                continue;
            }
            if !can_convert_from(mapping.effective_protocol(provider.protocol), protocol) {
                continue;
            }
            let key = (
                disabled_at,
                provider.id.trim().to_string(),
                mapping.local_model.trim().to_string(),
                upstream_model.clone(),
            );
            let replace = match &best {
                None => true,
                Some((best_key, _)) => key < *best_key,
            };
            if replace {
                let target = MappingTarget::new(&provider.id, &mapping.local_model, &upstream_model);
                best = Some((
                    key,
                    ProbeCandidate {
                        provider: provider.clone(),
                        target,
                        upstream_model,
                        protocol: mapping.effective_protocol(provider.protocol),
                    },
                ));
            }
        }
    }
    best.map(|(_, candidate)| candidate)
}

/// Process-wide set of mapping targets with a probe currently in flight.
fn active_probes() -> &'static Mutex<HashSet<MappingTarget>> {
    static ACTIVE_PROBES: OnceLock<Mutex<HashSet<MappingTarget>>> = OnceLock::new();
    ACTIVE_PROBES.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Single-flight guard for one in-flight probe of a mapping target.
///
/// Process memory only; dropping it releases the target, including when the
/// request future is cancelled, because the release runs in [`Drop`].
pub(in crate::ai_gateway) struct ProbeGuard {
    target: MappingTarget,
}

impl Drop for ProbeGuard {
    fn drop(&mut self) {
        active_probes()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.target);
    }
}

/// Try to become the single in-flight probe for `target`'s mapping key.
///
/// Returns `None` when another request already holds the guard, in which case
/// the caller must skip probing and continue on its exhausted path.
pub(in crate::ai_gateway) fn try_acquire_probe_guard(
    target: &MappingTarget,
) -> Option<ProbeGuard> {
    let mut active = active_probes()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if active.insert(target.clone()) {
        Some(ProbeGuard {
            target: target.clone(),
        })
    } else {
        None
    }
}
