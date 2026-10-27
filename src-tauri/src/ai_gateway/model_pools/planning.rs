use super::candidate::{expand_pool_fallback_groups, resolve_pool_candidates};
use super::health::pool_candidate_cooling;
use super::scheduler::{fallback_queue, pool_weighted_order};
#[cfg(test)]
use super::scheduler::note_pool_provider_copy;
use super::super::selection::{find_key_probe_candidate, find_probe_candidate, MappingTarget, ProbeCandidate};
use super::super::{
    GatewayConfig, GatewayUpstreamProvider, ModelPoolCandidate, PoolNativeReasoning,
    UpstreamProtocol,
};
use std::collections::HashSet;

/// The plan for one request whose model is a current or retired pool alias.
pub(in crate::ai_gateway) enum PoolPlan {
    /// The pool has at least one usable selected candidate (or an eligible
    /// probe), or at least one reachable fallback source. `ordered` is the pool
    /// SWRR order and never contains an ordinary or unselected source; the
    /// fallback queue is carried separately and never enters that order.
    Ready {
        ordered: Vec<GatewayUpstreamProvider>,
        probe: Option<ProbeCandidate>,
        fallback: Vec<PoolFallbackStage>,
    },
    /// The alias fails closed before any ordinary route is considered.
    Fenced { message: String },
    /// The request is rejected before any selection, scheduler, key, probe or
    /// health mutation: an unsupported preference, a declared-but-unavailable
    /// preference, or an output-cap-incompatible preference.
    Rejected {
        status: u16,
        code: &'static str,
        message: String,
    },
}

/// One reachable fallback source. A ready source is a single prepared provider
/// pass; a probe variant is the one shared half-open recovery attempt for an
/// auto-disabled fallback mapping. Neither variant is part of the primary
/// weighted order, and the request consumes the queue in stored model order.
#[derive(Clone)]
pub(in crate::ai_gateway) enum PoolFallbackStage {
    Ready(GatewayUpstreamProvider),
    Probe(ProbeCandidate),
}

/// Plan a pool request for `alias`. Returns `None` when the alias is neither a
/// current pool model nor a reserved retired alias, so the caller keeps the
/// ordinary route.
pub(in crate::ai_gateway) fn plan_pool_request(
    config: &GatewayConfig,
    alias: &str,
    protocol: UpstreamProtocol,
    request: &super::super::reasoning::PoolReasoningRequest,
    now: u64,
) -> Option<PoolPlan> {
    let alias = alias.trim();
    let Some(pool) = config
        .model_pools
        .iter()
        .find(|pool| pool.model.trim() == alias)
    else {
        if is_reserved_alias(config, alias) {
            return Some(PoolPlan::Fenced {
                message: format!(
                    "model '{alias}' is a retired model-pool alias and is not available"
                ),
            });
        }
        return None;
    };
    if protocol == UpstreamProtocol::AnthropicMessages {
        return Some(PoolPlan::Fenced {
            message: format!(
                "model pool '{alias}' is available only through the OpenAI Chat/Responses endpoints"
            ),
        });
    }
    if !pool.enabled {
        return Some(PoolPlan::Fenced {
            message: format!("model pool '{alias}' is disabled"),
        });
    }
    // Resolve each candidate's native reasoning control (or rejection) before
    // any scheduler/health mutation. The configured union spans the regular
    // candidates and every auto-expanded fallback source. An `auto` preference
    // keeps every candidate and injects no control.
    let preference = super::super::reasoning::resolve_pool_preference(pool, request.requested);
    let (fallback_groups, fallback_candidates) = expand_pool_fallback_groups(config, pool);
    let mut natives: Vec<Option<PoolNativeReasoning>> = Vec::with_capacity(pool.candidates.len());
    let mut fallback_natives: Vec<Option<PoolNativeReasoning>> =
        Vec::with_capacity(fallback_candidates.len());
    if let Some(preference) = preference {
        let mut declarations = 0usize;
        let mut valid = 0usize;
        let mut compatible = 0usize;
        let mut cap_budget: Option<u32> = None;
        let mut reasons: Vec<String> = Vec::new();
        let mut classify =
            |candidate: &ModelPoolCandidate| match super::super::reasoning::classify_candidate_reasoning(
                config,
                candidate,
                preference,
                &request.cap,
            ) {
                super::super::reasoning::CandidateReasoning::Unmapped => None,
                super::super::reasoning::CandidateReasoning::Invalid(reason) => {
                    declarations += 1;
                    reasons.push(reason);
                    None
                }
                super::super::reasoning::CandidateReasoning::CapIncompatible { budget } => {
                    declarations += 1;
                    valid += 1;
                    cap_budget.get_or_insert(budget);
                    None
                }
                super::super::reasoning::CandidateReasoning::Control(control) => {
                    declarations += 1;
                    valid += 1;
                    compatible += 1;
                    Some(control)
                }
            };
        for candidate in &pool.candidates {
            let native = classify(candidate);
            natives.push(native);
        }
        for candidate in &fallback_candidates {
            let native = classify(candidate);
            fallback_natives.push(native);
        }
        if declarations == 0 {
            return Some(PoolPlan::Rejected {
                status: 400,
                code: "unsupported_reasoning_effort",
                message: format!(
                    "model pool '{alias}' does not support reasoning effort '{}': no selected candidate or fallback source declares a mapping for it",
                    preference.as_str()
                ),
            });
        }
        if valid == 0 {
            let detail = if reasons.is_empty() {
                "no candidate mapping is currently valid".to_string()
            } else {
                reasons.join("; ")
            };
            return Some(PoolPlan::Rejected {
                status: 502,
                code: "all_providers_unavailable",
                message: format!(
                    "model pool '{alias}' has no candidate that can serve reasoning effort '{}' ({detail})",
                    preference.as_str()
                ),
            });
        }
        if compatible == 0 {
            let budget = cap_budget.unwrap_or(0);
            return Some(PoolPlan::Rejected {
                status: 400,
                code: "reasoning_output_limit_incompatible",
                message: format!(
                    "field '{}': the configured reasoning budget {budget} is not below the request's effective output limit {}",
                    request.cap.field, request.cap.tokens
                ),
            });
        }
    } else {
        natives = pool
            .candidates
            .iter()
            .map(|candidate| Some(PoolNativeReasoning::provider_default(candidate.protocol)))
            .collect();
        fallback_natives = fallback_candidates
            .iter()
            .map(|candidate| Some(PoolNativeReasoning::provider_default(candidate.protocol)))
            .collect();
    }
    let resolved =
        resolve_pool_candidates(config, pool, alias, Some(protocol), Some(&natives), now);
    // Healthy candidates: usable key, enabled provider/mapping, supported
    // protocol, not cooling and able to serve the resolved preference. Only
    // these consume a primary SWRR slot.
    let healthy: Vec<&super::candidate::ResolvedCandidate> = resolved
        .iter()
        .filter(|candidate| {
            candidate.available && candidate.prepared.is_some() && candidate.native.is_some()
        })
        .collect();
    let mut ordered = pool_weighted_order(&healthy, &pool.id, protocol.endpoint_path(), None);
    // A selected mapping whose only usable-key state is an eligible quota-key
    // probe is appended after the healthy primaries (in candidate order), so its
    // at-most-one key probe stays reachable without consuming a primary slot.
    for candidate in &resolved {
        if candidate.available
            || candidate.availability != "no_usable_key"
            || candidate.prepared.is_none()
            || candidate.native.is_none()
        {
            continue;
        }
        let Some(prepared) = candidate.prepared.as_ref() else {
            continue;
        };
        if find_key_probe_candidate(prepared, now).is_some() {
            let cloned = prepared.clone();
            #[cfg(test)]
            note_pool_provider_copy();
            ordered.push(cloned);
        }
    }
    // At most one half-open probe (mapping or, only when no mapping probe is
    // eligible, a selected auto-disabled mapping) is permitted per pool request.
    let probe = pool_probe(&resolved, alias, protocol, now);
    // The expanded fallback list is a distinct last stage: it is never appended
    // to the primary weighted order or the primary probe. Each stored model
    // contributes its own SWRR order, and the groups stay concatenated so every
    // source of an earlier model precedes every source of a later one. A mapping
    // probe is offered only while the regular stage has no probe of its own.
    let fallback = fallback_queue(
        config,
        pool,
        alias,
        protocol,
        &fallback_candidates,
        &fallback_natives,
        &fallback_groups,
        probe.is_none(),
        now,
    );
    if ordered.is_empty() && probe.is_none() && fallback.is_empty() {
        return Some(PoolPlan::Fenced {
            message: pool_unavailable_message(alias, &resolved),
        });
    }
    Some(PoolPlan::Ready {
        ordered,
        probe,
        fallback,
    })
}

/// Whether `alias` is a reserved retired alias, so it must never fall through
/// to an ordinary provider or default.
fn is_reserved_alias(config: &GatewayConfig, alias: &str) -> bool {
    let alias = alias.trim();
    config
        .model_pool_reserved_models
        .iter()
        .any(|reserved| reserved.trim() == alias)
}

/// Every alias an ordinary provider mapping must not acquire: the current model
/// ids of every pool plus the retired reservation list.
pub(in crate::ai_gateway) fn reserved_aliases(config: &GatewayConfig) -> HashSet<String> {
    let mut aliases: HashSet<String> = config
        .model_pools
        .iter()
        .map(|pool| pool.model.trim().to_string())
        .filter(|alias| !alias.is_empty())
        .collect();
    for reserved in &config.model_pool_reserved_models {
        let reserved = reserved.trim();
        if !reserved.is_empty() {
            aliases.insert(reserved.to_string());
        }
    }
    aliases
}

/// The nonempty fail-closed diagnostic naming each stored reference and why it
/// could not serve, so an exhausted pool never returns an empty error.
fn pool_unavailable_message(alias: &str, resolved: &[super::candidate::ResolvedCandidate]) -> String {
    if resolved.is_empty() {
        return format!("model pool '{alias}' has no selected candidates");
    }
    let details = resolved
        .iter()
        .map(|candidate| {
            format!(
                "{} -> {}: {}",
                candidate.identity.local_model,
                candidate.identity.upstream_model,
                candidate.reason.as_deref().unwrap_or("unavailable"),
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!("model pool '{alias}' has no available candidate ({details})")
}

/// The single eligible half-open probe inside the pool's own candidate set.
///
/// The prepared clones expose the auto-disabled rows the normal pass skips; the
/// returned target is rewritten to the real original mapping so the probe
/// settles on the real source row, and the probe never widens beyond the pool.
fn pool_probe(
    resolved: &[super::candidate::ResolvedCandidate],
    alias: &str,
    _protocol: UpstreamProtocol,
    now: u64,
) -> Option<ProbeCandidate> {
    // A candidate that is still cooling must not be probed; the cooldown is
    // shared across ingresses for that candidate.
    //
    // Each selected candidate is probed through the ordinary eligibility
    // predicate under its *own stored source protocol*, so a cross-OpenAI
    // selected mapping (e.g. a Responses row reached via the Chat ingress)
    // stays reachable without loosening the ordinary selection protocol filter
    // or relabelling the source. The existing per-provider tie/cooldown/auth
    // rules stay inside `find_probe_candidate`; this loop only picks the
    // earliest candidate exactly as the ordinary single call did.
    let mut best: Option<((u64, String, String, String), ProbeCandidate)> = None;
    for candidate in resolved {
        // A candidate that cannot serve the resolved preference is excluded from
        // the request entirely, including the half-open probe.
        if candidate.native.is_none() {
            continue;
        }
        if pool_candidate_cooling(&candidate.identity).is_some() {
            continue;
        }
        let Some(prepared) = candidate.prepared.as_ref() else {
            continue;
        };
        let Some(probe) = find_probe_candidate(
            std::slice::from_ref(prepared),
            Some(alias),
            candidate.identity.protocol,
            &[],
            now,
        ) else {
            continue;
        };
        let disabled_at = prepared
            .mappings
            .iter()
            .filter_map(|mapping| mapping.disabled_at)
            .min()
            .unwrap_or(0);
        let key = (
            disabled_at,
            probe.provider.id.trim().to_string(),
            candidate.identity.local_model.trim().to_string(),
            candidate.identity.upstream_model.trim().to_string(),
        );
        let replace = best.as_ref().is_none_or(|(best_key, _)| key < *best_key);
        if replace {
            best = Some((key, probe));
        }
    }
    let (_, mut probe) = best?;
    if let Some(identity) = probe.provider.pool_candidate.as_ref() {
        probe.target = MappingTarget::new(
            &identity.provider_id,
            &identity.local_model,
            &identity.upstream_model,
        );
    }
    Some(probe)
}
