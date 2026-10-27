use super::health::pool_candidate_cooling;
#[cfg(test)]
use super::scheduler::note_pool_provider_copy;
use super::super::selection::{can_convert_from, select_usable_key};
use super::super::{
    GatewayConfig, GatewayUpstreamProvider, ModelMapping, ModelPool, ModelPoolCandidate,
    PoolCandidateIdentity, PoolNativeReasoning, UpstreamProtocol,
};
use std::collections::HashSet;

/// One resolved pool candidate: its exact stored identity, the prepared routing
/// provider clone, and its current nonsecret availability.
#[derive(Clone)]
pub(in crate::ai_gateway) struct ResolvedCandidate {
    pub(in crate::ai_gateway) identity: PoolCandidateIdentity,
    pub(in crate::ai_gateway) weight: u32,
    pub(in crate::ai_gateway) prepared: Option<GatewayUpstreamProvider>,
    pub(in crate::ai_gateway) available: bool,
    /// Nonsecret availability code: `ready`, `cooling`, `source_disabled`,
    /// `mapping_unavailable`, `missing_provider`, `no_usable_key` or
    /// `unsupported_protocol`.
    pub(in crate::ai_gateway) availability: &'static str,
    pub(in crate::ai_gateway) reason: Option<String>,
    pub(in crate::ai_gateway) cooling_until: Option<u64>,
    /// The pool-only native reasoning control resolved for this candidate and
    /// request; `None` when the candidate cannot serve the resolved preference.
    pub(in crate::ai_gateway) native: Option<PoolNativeReasoning>,
}

/// Whether a stored candidate protocol can serve an inbound OpenAI/Chat request
/// for a model pool.
///
/// The ordinary selection boundary (`can_convert_from`) only accepts the same
/// protocol or a native Anthropic target; a pool additionally admits the two
/// OpenAI endpoints in either direction because Step 2 translates between them
/// per attempt. Native Messages ingress never reaches here: a Messages request
/// for a pool alias is fenced before candidate resolution.
fn pool_ingress_supported(candidate: UpstreamProtocol, ingress: UpstreamProtocol) -> bool {
    let candidate_openai = matches!(
        candidate,
        UpstreamProtocol::ChatCompletions | UpstreamProtocol::Responses
    );
    let ingress_openai = matches!(
        ingress,
        UpstreamProtocol::ChatCompletions | UpstreamProtocol::Responses
    );
    can_convert_from(candidate, ingress) || (candidate_openai && ingress_openai)
}

/// Resolve one pool candidate against the latest configuration.
///
/// `ingress` is the inbound protocol; a candidate whose stored effective
/// protocol cannot serve it is reported unavailable (Step 1's same-protocol
/// transport baseline). `None` is used by the UI status path, which reports the
/// reference's own readiness independently of any one request.
pub(super) fn resolve_candidate(
    config: &GatewayConfig,
    pool: &ModelPool,
    alias: &str,
    candidate: &ModelPoolCandidate,
    ingress: Option<UpstreamProtocol>,
    native: Option<PoolNativeReasoning>,
    now: u64,
) -> ResolvedCandidate {
    let identity = PoolCandidateIdentity {
        pool_id: pool.id.clone(),
        provider_id: candidate.provider_id.trim().to_string(),
        local_model: candidate.local_model.trim().to_string(),
        upstream_model: candidate.upstream_model.trim().to_string(),
        protocol: candidate.protocol,
        reasoning: native.clone(),
    };
    let mut prepared = None;
    let mut available = false;
    let mut availability: &'static str;
    let mut reason: Option<String>;
    match config
        .providers
        .iter()
        .find(|provider| provider.id == identity.provider_id)
    {
        None => {
            availability = "missing_provider";
            reason = Some(format!("missing provider '{}'", identity.provider_id));
        }
        Some(provider) => {
            let mapping = provider.mappings.iter().find(|mapping| {
                mapping.local_model.trim() == identity.local_model
                    && mapping.upstream_model.trim() == identity.upstream_model
            });
            match mapping {
                None => {
                    availability = "mapping_unavailable";
                    reason = Some(format!(
                        "mapping '{} -> {}' is missing or changed",
                        identity.local_model, identity.upstream_model
                    ));
                }
                Some(mapping) => {
                    if mapping.effective_protocol(provider.protocol) != identity.protocol {
                        availability = "mapping_unavailable";
                        reason = Some("mapping protocol changed".to_string());
                    } else {
                        // Only a candidate that can actually be selected or probed
                        // needs its one-mapping routing view. Disabled sources stay
                        // borrowed so their count never scales with the stored set.
                        if provider.enabled && mapping.enabled {
                            prepared = Some(prepare_provider(
                                provider,
                                alias,
                                mapping,
                                &identity,
                                candidate.weight,
                            ));
                        }
                        if !provider.enabled {
                            availability = "source_disabled";
                            reason = Some("source provider disabled".to_string());
                        } else if !mapping.enabled {
                            availability = "mapping_unavailable";
                            reason = Some("source mapping disabled".to_string());
                        } else if mapping.auto_disabled {
                            availability = "mapping_unavailable";
                            reason = Some("source mapping unavailable".to_string());
                        } else if ingress.is_some_and(|protocol| {
                            !pool_ingress_supported(identity.protocol, protocol)
                        }) {
                            availability = "unsupported_protocol";
                            reason = Some("protocol not supported by this endpoint".to_string());
                        } else if select_usable_key(provider, now, &HashSet::new()).is_none() {
                            availability = "no_usable_key";
                            reason = Some("no usable upstream key".to_string());
                        } else {
                            available = true;
                            availability = "ready";
                            reason = None;
                        }
                    }
                }
            }
        }
    }
    let mut cooling_until = None;
    if available {
        if let Some(until) = pool_candidate_cooling(&identity) {
            available = false;
            cooling_until = Some(until);
            availability = "cooling";
            reason = Some("cooling after a model-scoped failure".to_string());
        }
    }
    ResolvedCandidate {
        identity,
        weight: candidate.weight,
        prepared,
        available,
        availability,
        reason,
        cooling_until,
        native,
    }
}

/// Build the one-mapping routing clone for a candidate. The clone carries the
/// real provider, key pool and endpoint plus a transport-only candidate identity;
/// the local model is rewritten to the pool alias so the ordinary resolution
/// serves the exact original upstream model, and `default_model` is cleared so
/// no unselected sibling can ever be selected.
fn prepare_provider(
    provider: &GatewayUpstreamProvider,
    alias: &str,
    mapping: &ModelMapping,
    identity: &PoolCandidateIdentity,
    weight: u32,
) -> GatewayUpstreamProvider {
    let mut prepared = provider.clone();
    #[cfg(test)]
    note_pool_provider_copy();
    let mut row = mapping.clone();
    row.local_model = alias.to_string();
    row.protocol = Some(identity.protocol);
    prepared.mappings = vec![row];
    prepared.default_model = None;
    prepared.weight = weight;
    prepared.attempt_key = String::new();
    prepared.pool_candidate = Some(identity.clone());
    prepared
}

/// Resolve every stored candidate of one pool against the latest configuration.
pub(super) fn resolve_pool_candidates(
    config: &GatewayConfig,
    pool: &ModelPool,
    alias: &str,
    ingress: Option<UpstreamProtocol>,
    natives: Option<&[Option<PoolNativeReasoning>]>,
    now: u64,
) -> Vec<ResolvedCandidate> {
    pool.candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let native = natives
                .and_then(|natives| natives.get(index))
                .and_then(|native| native.clone());
            resolve_candidate(config, pool, alias, candidate, ingress, native, now)
        })
        .collect()
}

/// Expand one stored fallback gateway model to every provider mapping that
/// serves its trimmed local model, preserving provider and mapping order.
///
/// Each expanded reference carries the exact provider, local/upstream pair and
/// effective protocol, plus the native reasoning declaration derived from that
/// mapping. The same exact identity is emitted once. Disabled providers and
/// mappings are included so the status path can report their availability; the
/// request path applies the ordinary source eligibility contract afterwards.
pub(in crate::ai_gateway) fn expand_fallback_model_in(
    providers: &[GatewayUpstreamProvider],
    local_model: &str,
) -> Vec<ModelPoolCandidate> {
    let local_model = local_model.trim();
    if local_model.is_empty() {
        return Vec::new();
    }
    let mut seen: Vec<(String, String, String, &'static str)> = Vec::new();
    let mut expanded = Vec::new();
    for provider in providers {
        let provider_id = provider.id.trim();
        for mapping in &provider.mappings {
            if mapping.local_model.trim() != local_model {
                continue;
            }
            let upstream_model = mapping.upstream_model.trim();
            if upstream_model.is_empty() {
                continue;
            }
            let protocol = mapping.effective_protocol(provider.protocol);
            let identity = (
                provider_id.to_string(),
                local_model.to_string(),
                upstream_model.to_string(),
                protocol.endpoint_path(),
            );
            if seen.contains(&identity) {
                continue;
            }
            seen.push(identity);
            expanded.push(ModelPoolCandidate {
                provider_id: provider_id.to_string(),
                local_model: local_model.to_string(),
                upstream_model: upstream_model.to_string(),
                protocol,
                weight: 1,
                reasoning: super::super::reasoning::auto_candidate_reasoning(mapping, protocol),
            });
        }
    }
    expanded
}

/// Expand a pool's ordered fallback model list into ordered per-model groups.
/// Each group keeps the stored model id and the flattened candidate range, so
/// every source of the first stored model precedes every source of the next.
pub(super) fn expand_pool_fallback_groups(
    config: &GatewayConfig,
    pool: &ModelPool,
) -> (Vec<(String, usize, usize)>, Vec<ModelPoolCandidate>) {
    let mut groups = Vec::new();
    let mut expanded = Vec::new();
    for model in &pool.fallback_models {
        let model = model.trim().to_string();
        let candidates = expand_fallback_model_in(&config.providers, &model);
        let start = expanded.len();
        let len = candidates.len();
        expanded.extend(candidates);
        groups.push((model, start, len));
    }
    (groups, expanded)
}
