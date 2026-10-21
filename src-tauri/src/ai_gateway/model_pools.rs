//! Operator-defined model pools (Step 1 of 20261013-free-model-pool).
//!
//! A model pool is a virtual provider that exposes one public model id (`model`)
//! while routing each request, through the existing gateway attempt pipeline, to
//! exactly one of its explicitly selected source mappings. This module owns the
//! pool-specific decisions the ordinary pipeline does not:
//!
//! - pool alias resolution, including the fail-closed current/retired boundary;
//! - exact source-candidate resolution against the latest configuration;
//! - pool-scoped smooth weighted round robin isolated from the ordinary
//!   scheduler and from session affinity;
//! - a process-memory candidate cooldown keyed by pool plus full candidate
//!   identity, so a model-scoped limit never marks a shared provider key;
//! - nonsecret readiness status for the UI;
//! - atomic create/edit validation and the alias reservation bookkeeping.
//!
//! Transport, streaming, first-byte and cancellation semantics stay in
//! `runtime_http.rs`; key selection/retry/probe decisions stay in
//! `attempt_policy.rs`. This module only decides which real sources a pool may
//! use and how a candidate-scoped failure cools one of them.

use super::selection::{
    can_convert_from, find_key_probe_candidate, find_probe_candidate, is_quota_exceeded_message,
    select_usable_key, MappingTarget, ProbeCandidate,
};
use super::{
    now_ts, GatewayConfig, GatewayUpstreamProvider, ModelMapping, ModelPool, ModelPoolCandidate,
    PoolCandidateIdentity, PoolNativeReasoning, UpstreamProtocol, MAX_PROVIDER_WEIGHT,
    MIN_PROVIDER_WEIGHT,
};
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::time::Instant;

/// Candidate-scoped cooldown applied when an upstream names no delay. Matches
/// the documented 60-second boundary; a valid header delay overrides it.
pub(in crate::ai_gateway) const POOL_CANDIDATE_COOLDOWN_SECS: u64 = 60;

/// Composite identity of one pool candidate, used for the candidate-scoped
/// cooldown table. Protocol is stored as the canonical endpoint suffix so it
/// stays a stable, hashable `&'static str`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PoolCandidateKey {
    pool_id: String,
    provider_id: String,
    local_model: String,
    upstream_model: String,
    protocol: &'static str,
}

/// Scheduler accounting key: one complete candidate identity plus the *ingress*
/// protocol. Two ingresses route to the same stored candidate under different
/// keys, so each ingress owns an independent cycle; the cooldown below stays
/// keyed by the candidate alone and is therefore shared across ingresses for
/// that candidate.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PoolSchedulerKey {
    candidate: PoolCandidateKey,
    ingress: &'static str,
    /// `None` for the primary pool order; the stored fallback model id for the
    /// fallback queue. A fallback source therefore never shares accounting with
    /// the primary order or with another fallback model.
    fallback_model: Option<String>,
}

fn candidate_key(pool_id: &str, identity: &PoolCandidateIdentity) -> PoolCandidateKey {
    PoolCandidateKey {
        pool_id: pool_id.trim().to_string(),
        provider_id: identity.provider_id.trim().to_string(),
        local_model: identity.local_model.trim().to_string(),
        upstream_model: identity.upstream_model.trim().to_string(),
        protocol: identity.protocol.endpoint_path(),
    }
}

/// Cooldown/scheduler key of one stored candidate reference in a pool.
fn candidate_key_of(pool_id: &str, candidate: &ModelPoolCandidate) -> PoolCandidateKey {
    PoolCandidateKey {
        pool_id: pool_id.trim().to_string(),
        provider_id: candidate.provider_id.trim().to_string(),
        local_model: candidate.local_model.trim().to_string(),
        upstream_model: candidate.upstream_model.trim().to_string(),
        protocol: candidate.protocol.endpoint_path(),
    }
}

fn scheduler_key(
    pool_id: &str,
    identity: &PoolCandidateIdentity,
    ingress: &'static str,
) -> PoolSchedulerKey {
    PoolSchedulerKey {
        candidate: candidate_key(pool_id, identity),
        ingress,
        fallback_model: None,
    }
}

fn fallback_scheduler_key(
    pool_id: &str,
    identity: &PoolCandidateIdentity,
    ingress: &'static str,
    fallback_model: &str,
) -> PoolSchedulerKey {
    PoolSchedulerKey {
        candidate: candidate_key(pool_id, identity),
        ingress,
        fallback_model: Some(fallback_model.trim().to_string()),
    }
}

/// A deterministic tie-break token for one candidate; two same-provider models
/// stay independent because the original local/upstream pair is part of it.
fn candidate_sort_token(identity: &PoolCandidateIdentity) -> String {
    format!(
        "{}|{}|{}|{}",
        identity.provider_id.trim(),
        identity.local_model.trim(),
        identity.upstream_model.trim(),
        identity.protocol.endpoint_path(),
    )
}

static POOL_SCHEDULER: OnceLock<Mutex<HashMap<PoolSchedulerKey, i64>>> = OnceLock::new();
/// Process-memory candidate cooldown. The value is a monotonic deadline
/// (`tokio::time::Instant`) so zero and subsecond Retry-After delays are honored
/// exactly and availability never depends on a wall-clock second boundary.
static POOL_COOLDOWN: OnceLock<Mutex<HashMap<PoolCandidateKey, Instant>>> = OnceLock::new();

fn pool_scheduler() -> &'static Mutex<HashMap<PoolSchedulerKey, i64>> {
    POOL_SCHEDULER.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pool_cooldown() -> &'static Mutex<HashMap<PoolCandidateKey, Instant>> {
    POOL_COOLDOWN.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Test-only thread-local count of deep `GatewayUpstreamProvider` copies made
/// while preparing and ordering pool candidates. Shared/`Arc` references never
/// increment it.
#[cfg(test)]
thread_local! {
    static POOL_PROVIDER_DEEP_COPIES: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn note_pool_provider_copy() {
    POOL_PROVIDER_DEEP_COPIES.with(|count| count.set(count.get().saturating_add(1)));
}

/// Reset the test-only pool provider deep-copy counter on this thread.
#[cfg(test)]
pub(in crate::ai_gateway) fn reset_pool_provider_copy_count() {
    POOL_PROVIDER_DEEP_COPIES.with(|count| count.set(0));
}

/// The number of deep `GatewayUpstreamProvider` copies made while planning a pool
/// request on this thread since the last reset.
#[cfg(test)]
pub(in crate::ai_gateway) fn pool_provider_copy_count() -> usize {
    POOL_PROVIDER_DEEP_COPIES.with(|count| count.get())
}

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
fn resolve_candidate(
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
fn resolve_pool_candidates(
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
                reasoning: super::reasoning::auto_candidate_reasoning(mapping, protocol),
            });
        }
    }
    expanded
}

/// Expand a pool's ordered fallback model list into ordered per-model groups.
/// Each group keeps the stored model id and the flattened candidate range, so
/// every source of the first stored model precedes every source of the next.
fn expand_pool_fallback_groups(
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

/// Plan a pool request for `alias`. Returns `None` when the alias is neither a
/// current pool model nor a reserved retired alias, so the caller keeps the
/// ordinary route.
pub(in crate::ai_gateway) fn plan_pool_request(
    config: &GatewayConfig,
    alias: &str,
    protocol: UpstreamProtocol,
    request: &super::reasoning::PoolReasoningRequest,
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
    let preference = super::reasoning::resolve_pool_preference(pool, request.requested);
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
            |candidate: &ModelPoolCandidate| match super::reasoning::classify_candidate_reasoning(
                config,
                candidate,
                preference,
                &request.cap,
            ) {
                super::reasoning::CandidateReasoning::Unmapped => None,
                super::reasoning::CandidateReasoning::Invalid(reason) => {
                    declarations += 1;
                    reasons.push(reason);
                    None
                }
                super::reasoning::CandidateReasoning::CapIncompatible { budget } => {
                    declarations += 1;
                    valid += 1;
                    cap_budget.get_or_insert(budget);
                    None
                }
                super::reasoning::CandidateReasoning::Control(control) => {
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
    let healthy: Vec<&ResolvedCandidate> = resolved
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
fn pool_unavailable_message(alias: &str, resolved: &[ResolvedCandidate]) -> String {
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

/// Pool-scoped smooth weighted round robin for one ingress.
///
/// Accounting is keyed by pool id plus the complete candidate identity
/// (provider, original local/upstream pair and stored protocol) *and the ingress
/// protocol*, so two pools, two same-provider models, and the Chat/Responses
/// entries routing to the same stored candidate each own an independent cycle.
/// A `fallback_model` additionally isolates one stored fallback model's cycle
/// from the primary order and from every other fallback model. An unavailable
/// candidate is filtered before it can consume a slot. The returned primary
/// candidate is first, followed by the remaining candidates in descending
/// updated current-weight order.
fn pool_weighted_order(
    candidates: &[&ResolvedCandidate],
    pool_id: &str,
    ingress: &'static str,
    fallback_model: Option<&str>,
) -> Vec<GatewayUpstreamProvider> {
    if candidates.is_empty() {
        return Vec::new();
    }
    if candidates.len() == 1 {
        let prepared = candidates[0].prepared.clone();
        #[cfg(test)]
        {
            if prepared.is_some() {
                note_pool_provider_copy();
            }
        }
        return prepared.into_iter().collect();
    }
    let keys: Vec<PoolSchedulerKey> = candidates
        .iter()
        .map(|candidate| match fallback_model {
            Some(model) => {
                fallback_scheduler_key(pool_id, &candidate.identity, ingress, model)
            }
            None => scheduler_key(pool_id, &candidate.identity, ingress),
        })
        .collect();
    let tokens: Vec<String> = candidates
        .iter()
        .map(|candidate| candidate_sort_token(&candidate.identity))
        .collect();
    let mut map = pool_scheduler()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let total_weight: i64 = candidates
        .iter()
        .map(|candidate| candidate.weight.clamp(1, MAX_PROVIDER_WEIGHT) as i64)
        .sum();

    for (candidate, key) in candidates.iter().zip(keys.iter()) {
        let current = map.entry(key.clone()).or_insert(0);
        *current += candidate.weight.clamp(1, MAX_PROVIDER_WEIGHT) as i64;
    }

    let mut best_idx = 0;
    let mut best_val = (
        map.get(&keys[0]).copied().unwrap_or(0),
        Reverse(tokens[0].clone()),
    );
    for (idx, key) in keys.iter().enumerate().skip(1) {
        let val = (
            map.get(key).copied().unwrap_or(0),
            Reverse(tokens[idx].clone()),
        );
        if val > best_val {
            best_val = val;
            best_idx = idx;
        }
    }
    if let Some(current) = map.get_mut(&keys[best_idx]) {
        *current -= total_weight;
    }

    let mut remaining: Vec<usize> = (0..candidates.len())
        .filter(|idx| *idx != best_idx)
        .collect();
    remaining.sort_by(|a, b| {
        let wa = map.get(&keys[*a]).copied().unwrap_or(0);
        let wb = map.get(&keys[*b]).copied().unwrap_or(0);
        wb.cmp(&wa).then_with(|| tokens[*a].cmp(&tokens[*b]))
    });

    let mut ordered: Vec<GatewayUpstreamProvider> = Vec::with_capacity(candidates.len());
    if let Some(primary) = candidates[best_idx].prepared.clone() {
        #[cfg(test)]
        note_pool_provider_copy();
        ordered.push(primary);
    }
    for idx in remaining {
        if let Some(prepared) = candidates[idx].prepared.clone() {
            #[cfg(test)]
            note_pool_provider_copy();
            ordered.push(prepared);
        }
    }
    ordered
}

/// Build the ordered fallback queue from the stored model groups.
///
/// Every stored model contributes its own equal-weight smooth weighted round
/// robin order over its currently eligible sources, scoped to the pool, that
/// fallback model and the ingress protocol. Sources whose only usable-key state
/// is an eligible quota probe follow that model's ready sources, and the first
/// auto-disabled mapping may lend the request-wide mapping probe while the
/// regular stage has none. Groups stay concatenated, so a model's sources are
/// all attempted before the next model becomes eligible.
fn fallback_queue(
    config: &GatewayConfig,
    pool: &ModelPool,
    alias: &str,
    ingress: UpstreamProtocol,
    candidates: &[ModelPoolCandidate],
    natives: &[Option<PoolNativeReasoning>],
    groups: &[(String, usize, usize)],
    probe_allowed: bool,
    now: u64,
) -> Vec<PoolFallbackStage> {
    let mut queue = Vec::new();
    let mut mapping_probe_used = !probe_allowed;
    for (model, start, len) in groups {
        let end = (*start).saturating_add(*len).min(candidates.len());
        let Some(group_candidates) = candidates.get(*start..end) else {
            continue;
        };
        let Some(group_natives) = natives.get(*start..end) else {
            continue;
        };
        let resolved: Vec<ResolvedCandidate> = group_candidates
            .iter()
            .zip(group_natives.iter())
            .map(|(candidate, native)| {
                resolve_candidate(
                    config,
                    pool,
                    alias,
                    candidate,
                    Some(ingress),
                    native.clone(),
                    now,
                )
            })
            .collect();
        let healthy: Vec<&ResolvedCandidate> = resolved
            .iter()
            .filter(|candidate| {
                candidate.available && candidate.prepared.is_some() && candidate.native.is_some()
            })
            .collect();
        for prepared in pool_weighted_order(
            &healthy,
            &pool.id,
            ingress.endpoint_path(),
            Some(model.as_str()),
        ) {
            queue.push(PoolFallbackStage::Ready(prepared));
        }
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
                queue.push(PoolFallbackStage::Ready(cloned));
            }
        }
        if mapping_probe_used {
            continue;
        }
        let probe_source = resolved.iter().find(|candidate| {
            candidate.native.is_some()
                && candidate.prepared.is_some()
                && candidate.availability == "mapping_unavailable"
                && pool_candidate_cooling(&candidate.identity).is_none()
        });
        let Some(resolved) = probe_source else {
            continue;
        };
        let Some(prepared) = resolved.prepared.as_ref() else {
            continue;
        };
        let Some(mut candidate) = find_probe_candidate(
            std::slice::from_ref(prepared),
            Some(alias),
            resolved.identity.protocol,
            &[],
            now,
        ) else {
            continue;
        };
        candidate.target = MappingTarget::new(
            &resolved.identity.provider_id,
            &resolved.identity.local_model,
            &resolved.identity.upstream_model,
        );
        candidate.protocol = resolved.identity.protocol;
        queue.push(PoolFallbackStage::Probe(candidate));
        mapping_probe_used = true;
    }
    queue
}

/// The single eligible half-open probe inside the pool's own candidate set.
///
/// The prepared clones expose the auto-disabled rows the normal pass skips; the
/// returned target is rewritten to the real original mapping so the probe
/// settles on the real source row, and the probe never widens beyond the pool.
fn pool_probe(
    resolved: &[ResolvedCandidate],
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

/// Whether a candidate-scoped failure should cool just this candidate instead of
/// using the existing key/account policy.
///
/// A structured `FreeUsageLimitError` 429 is always candidate-scoped, even when
/// its message also mentions an account. Otherwise a model-scoped or
/// unknown-scope quota-like failure on 400/402/429 is candidate-scoped, while an
/// explicit funds/credits/budget exhaustion signal keeps the existing
/// key/account policy. A transport failure is never candidate scoped.
pub(in crate::ai_gateway) fn pool_candidate_scope_failure(
    status: u16,
    error_message: Option<&str>,
    error_type: Option<&str>,
    transport: bool,
) -> bool {
    if transport {
        return false;
    }
    if status == 429 && is_free_usage_limit_error(error_type) {
        return true;
    }
    match status {
        429 => !is_explicit_account_scope(error_message),
        400 | 402 => {
            is_quota_exceeded_message(error_message) && !is_explicit_account_scope(error_message)
        }
        _ => false,
    }
}

/// Whether the structured upstream error type is the free-usage limit signal.
/// Compared case-insensitively; only the structured `error.type` selects this,
/// never a bare message word.
pub(in crate::ai_gateway) fn is_free_usage_limit_error(error_type: Option<&str>) -> bool {
    error_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some_and(|value| value.eq_ignore_ascii_case("FreeUsageLimitError"))
}

/// Whether the sanitized upstream text explicitly names an account
/// funds/credits/budget *exhaustion*, in which case the existing account/key
/// quota policy applies instead of a candidate-scoped cooldown. A bare
/// `account`, `billing`, `invoice` or `payment` word is not an exhaustion signal.
fn is_explicit_account_scope(message: Option<&str>) -> bool {
    let Some(message) = message.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    let lower = message.to_ascii_lowercase();
    const SCOPE: [&str; 6] = ["credit", "funds", "fund", "budget", "balance", "prepaid"];
    const EXHAUSTED: [&str; 8] = [
        "exhaust",
        "insufficient",
        "deplet",
        "limit",
        "empty",
        "zero",
        "ran out",
        "out of",
    ];
    SCOPE.iter().any(|signal| lower.contains(signal))
        && EXHAUSTED.iter().any(|signal| lower.contains(signal))
}

/// Cool one candidate until `Instant::now() + delay`, where a valid header delay
/// wins and an absent/invalid delay falls back to
/// [`POOL_CANDIDATE_COOLDOWN_SECS`]. The exact `Duration` is preserved, so a
/// `retry-after-ms: 0` yields no cooldown at all and a long delay is stored
/// across requests instead of consuming a request wait budget. Process memory
/// only; nothing is persisted.
pub(in crate::ai_gateway) fn cool_pool_candidate(
    identity: &PoolCandidateIdentity,
    delay: Option<Duration>,
) {
    let delay = delay.unwrap_or(Duration::from_secs(POOL_CANDIDATE_COOLDOWN_SECS));
    let deadline = Instant::now() + delay;
    pool_cooldown()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(candidate_key(&identity.pool_id, identity), deadline);
}

/// The cooldown deadline of one candidate when it is still in the future.
fn cooldown_deadline(identity: &PoolCandidateIdentity) -> Option<Instant> {
    let key = candidate_key(&identity.pool_id, identity);
    let map = pool_cooldown()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    map.get(&key)
        .copied()
        .filter(|deadline| Instant::now() < *deadline)
}

/// The documented wall-clock cooldown deadline for UI status, in epoch seconds,
/// derived from the same monotonic deadline. `None` when the candidate is not
/// cooling (including a zero/short delay already elapsed). Seconds are rounded
/// up so the reported instant is never earlier than the real deadline; the
/// availability decision itself uses the monotonic deadline, not this value.
pub(in crate::ai_gateway) fn pool_candidate_cooling(
    identity: &PoolCandidateIdentity,
) -> Option<u64> {
    let deadline = cooldown_deadline(identity)?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    let seconds = remaining
        .as_secs()
        .saturating_add(u64::from(remaining.subsec_nanos() > 0));
    Some(now_ts().saturating_add(seconds))
}

/// Prune pool scheduler and cooldown entries that this publication removed. A
/// failed write never reaches this call.
///
/// Ownership is the union of regular candidate identities and every provider
/// mapping that serves a stored fallback model, resolved against the published
/// provider list. An identity kept in either role therefore survives, including
/// one reached both as a regular candidate and through a fallback model. When the
/// published configuration cannot be re-read, only removed pool ids are pruned
/// so runtime state is retained rather than guessed away.
///
/// The tables are process-global and can also hold entries for a configuration
/// outside the before/after pair, so an id is only considered when the pair
/// itself contains it. A pool that appears on neither side keeps all of its
/// scheduler and cooldown state.
pub(in crate::ai_gateway) fn prune_removed_pools(before: &[ModelPool], after: &[ModelPool]) {
    let touched_pools: HashSet<String> = before
        .iter()
        .chain(after.iter())
        .map(|pool| pool.id.trim().to_string())
        .filter(|pool_id| !pool_id.is_empty())
        .collect();
    if touched_pools.is_empty() {
        return;
    }
    let removed_pools: HashSet<String> = before
        .iter()
        .filter(|old| !after.iter().any(|new| new.id == old.id))
        .map(|pool| pool.id.trim().to_string())
        .collect();
    let published = super::storage::read_config_shared().ok();
    let owned: Option<HashSet<PoolCandidateKey>> = published.as_ref().map(|config| {
        config
            .model_pools
            .iter()
            .flat_map(|pool| owned_candidate_keys(pool, &config.providers))
            .collect()
    });
    {
        let mut scheduler = pool_scheduler()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Prune every ingress variant of a candidate this publication no longer
        // owns; leave every other configuration's entries untouched.
        scheduler.retain(|key, _| match owned.as_ref() {
            Some(owned) => {
                !touched_pools.contains(&key.candidate.pool_id)
                    || owned.contains(&key.candidate)
            }
            None => !removed_pools.contains(&key.candidate.pool_id),
        });
    }
    {
        let mut cooldown = pool_cooldown()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cooldown.retain(|key, _| match owned.as_ref() {
            Some(owned) => {
                !touched_pools.contains(&key.pool_id) || owned.contains(key)
            }
            None => !removed_pools.contains(&key.pool_id),
        });
    }
}

/// Every candidate identity one pool owns in the published configuration.
fn owned_candidate_keys(
    pool: &ModelPool,
    providers: &[GatewayUpstreamProvider],
) -> HashSet<PoolCandidateKey> {
    let mut owned: HashSet<PoolCandidateKey> = pool
        .candidates
        .iter()
        .map(|candidate| candidate_key_of(&pool.id, candidate))
        .collect();
    for model in &pool.fallback_models {
        for candidate in expand_fallback_model_in(providers, model) {
            owned.insert(candidate_key_of(&pool.id, &candidate));
        }
    }
    owned
}

/// The caller-facing diagnostic label for one failed attempt.
///
/// A model-pool attempt keeps the real provider name and appends the original
/// stored mapping identity, so two same-provider pool candidates never collapse
/// into one diagnostic entry. An ordinary attempt keeps exactly its provider
/// name, preserving the existing all-unavailable semantics.
pub(in crate::ai_gateway) fn failure_label(provider: &GatewayUpstreamProvider) -> String {
    match provider.pool_candidate.as_ref() {
        Some(identity) => format!(
            "{} ({} -> {})",
            provider.name, identity.local_model, identity.upstream_model
        ),
        None => provider.name.clone(),
    }
}

// ---------------------------------------------------------------------------
// Validation and UI status
// ---------------------------------------------------------------------------

/// Whether one new candidate reference exists in the latest configuration: the
/// provider, the original trimmed mapping pair and its effective protocol.
pub(in crate::ai_gateway) fn reference_resolves(
    config: &GatewayConfig,
    candidate: &ModelPoolCandidate,
) -> bool {
    reference_resolves_in(&config.providers, candidate)
}

/// [`reference_resolves`] against an explicit provider list, so a catalog path
/// that already holds the provider slice never needs the full configuration.
pub(in crate::ai_gateway) fn reference_resolves_in(
    providers: &[GatewayUpstreamProvider],
    candidate: &ModelPoolCandidate,
) -> bool {
    providers.iter().any(|provider| {
        provider.id.trim() == candidate.provider_id.trim()
            && provider.mappings.iter().any(|mapping| {
                mapping.local_model.trim() == candidate.local_model.trim()
                    && mapping.upstream_model.trim() == candidate.upstream_model.trim()
                    && mapping.effective_protocol(provider.protocol) == candidate.protocol
            })
    })
}

fn candidate_tuple(candidate: &ModelPoolCandidate) -> (String, String, String, UpstreamProtocol) {
    (
        candidate.provider_id.trim().to_string(),
        candidate.local_model.trim().to_string(),
        candidate.upstream_model.trim().to_string(),
        candidate.protocol,
    )
}

/// Validate and normalize a submitted pool against the latest configuration.
///
/// A new reference must resolve against the latest providers; a reference that
/// was already stored stays valid even when its source later becomes
/// unavailable, so the operator can retain it or explicitly remove it. The
/// submitted alias must not collide with an ordinary mapping row or another
/// current pool, and every candidate needs a nonblank tuple and a weight in
/// 1..=100.
pub(in crate::ai_gateway) fn validate_pool_upsert(
    config: &GatewayConfig,
    incoming: &ModelPool,
) -> Result<ModelPool, String> {
    let mut pool = incoming.clone();
    pool.id = pool.id.trim().to_string();
    pool.name = pool.name.trim().to_string();
    pool.model = pool.model.trim().to_string();
    if pool.name.is_empty() {
        return Err("model pool name must not be blank".to_string());
    }
    if pool.model.is_empty() {
        return Err("model pool public model id must not be blank".to_string());
    }
    if pool.candidates.is_empty() {
        return Err("a model pool requires at least one candidate".to_string());
    }
    let mut seen: Vec<(String, String, String, UpstreamProtocol)> = Vec::new();
    for candidate in &mut pool.candidates {
        candidate.provider_id = candidate.provider_id.trim().to_string();
        candidate.local_model = candidate.local_model.trim().to_string();
        candidate.upstream_model = candidate.upstream_model.trim().to_string();
        if candidate.provider_id.is_empty() {
            return Err("a model pool candidate requires a provider id".to_string());
        }
        if candidate.local_model.is_empty() || candidate.upstream_model.is_empty() {
            return Err(
                "a model pool candidate requires a nonblank local and upstream model".to_string(),
            );
        }
        if candidate.weight < MIN_PROVIDER_WEIGHT || candidate.weight > MAX_PROVIDER_WEIGHT {
            return Err(format!(
                "candidate weight must be between {MIN_PROVIDER_WEIGHT} and {MAX_PROVIDER_WEIGHT}"
            ));
        }
        let tuple = candidate_tuple(candidate);
        if seen.contains(&tuple) {
            return Err(format!(
                "duplicate model pool candidate '{} -> {}'",
                candidate.local_model, candidate.upstream_model
            ));
        }
        seen.push(tuple);
    }
    // The ordered fallback list carries gateway local model ids only. Every
    // entry is trimmed, nonblank and unique, may not name a current or retired
    // pool alias, and a newly submitted entry must resolve to at least one
    // provider mapping. An unchanged saved entry stays retainable and clearable
    // after its source disappears.
    let forbidden = {
        let mut aliases = reserved_aliases(config);
        aliases.insert(pool.model.clone());
        aliases
    };
    let existing_pool = config
        .model_pools
        .iter()
        .find(|stored| stored.id == pool.id);
    let mut fallback_models: Vec<String> = Vec::with_capacity(pool.fallback_models.len());
    for model in &pool.fallback_models {
        let model = model.trim().to_string();
        if model.is_empty() {
            return Err("a model pool fallback model must not be blank".to_string());
        }
        if fallback_models.contains(&model) {
            return Err(format!(
                "duplicate model pool fallback model '{model}'"
            ));
        }
        if forbidden.contains(&model) {
            return Err(format!(
                "model pool fallback model '{model}' is a model-pool alias and cannot recurse into a pool"
            ));
        }
        if expand_fallback_model_in(&config.providers, &model).is_empty() {
            let retained = existing_pool.is_some_and(|stored| {
                stored
                    .fallback_models
                    .iter()
                    .any(|stored| stored.trim() == model)
            });
            if !retained {
                return Err(format!(
                    "model pool fallback model '{model}' is not served by any provider mapping"
                ));
            }
        }
        fallback_models.push(model);
    }
    pool.fallback_models = fallback_models;
    for provider in &config.providers {
        for mapping in &provider.mappings {
            if mapping.local_model.trim() == pool.model {
                return Err(format!(
                    "public model id '{}' conflicts with an existing provider mapping",
                    pool.model
                ));
            }
        }
    }
    for other in &config.model_pools {
        if other.id != pool.id && other.model.trim() == pool.model {
            return Err(format!(
                "public model id '{}' is already used by another model pool",
                pool.model
            ));
        }
    }
    let existing = existing_pool;
    for candidate in &pool.candidates {
        if reference_resolves(config, candidate) {
            continue;
        }
        let retained = existing.is_some_and(|stored| {
            stored
                .candidates
                .iter()
                .any(|other| candidate_tuple(other) == candidate_tuple(candidate))
        });
        if !retained {
            return Err(format!(
                "model pool candidate references an unknown provider/mapping/protocol: {} -> {} ({})",
                candidate.local_model,
                candidate.upstream_model,
                candidate.protocol.endpoint_path()
            ));
        }
    }
    super::reasoning::validate_pool_reasoning(config, existing, &pool)?;
    Ok(pool)
}

/// Push one retired alias into the reservation list without duplicating it.
pub(in crate::ai_gateway) fn reserve_alias(config: &mut GatewayConfig, alias: &str) {
    let alias = alias.trim();
    if alias.is_empty() {
        return;
    }
    if !config
        .model_pool_reserved_models
        .iter()
        .any(|reserved| reserved.trim() == alias)
    {
        config.model_pool_reserved_models.push(alias.to_string());
    }
}

/// Generate a stable pool identity.
pub(in crate::ai_gateway) fn new_pool_id() -> String {
    format!("gwpool-{}", uuid::Uuid::new_v4().simple())
}

/// Nonsecret readiness of one stored pool candidate for the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ModelPoolCandidateStatus {
    pub provider_id: String,
    pub provider_name: Option<String>,
    pub local_model: String,
    pub upstream_model: String,
    pub protocol: UpstreamProtocol,
    pub weight: u32,
    pub ready: bool,
    pub availability: String,
    pub reason: Option<String>,
    pub cooling_until: Option<u64>,
}

/// One canonical row of a pool's static reasoning coverage (REQ-005). The count
/// is configuration-derived and never reflects cooldown, key availability,
/// health or source enable state.
#[derive(Debug, Clone, Serialize)]
pub struct ModelPoolReasoningCoverage {
    pub effort: String,
    pub mapped_candidates: usize,
    pub total_candidates: usize,
}

/// Nonsecret readiness of one expanded provider mapping serving a fallback
/// gateway model. It carries no key, endpoint, header or message body.
#[derive(Debug, Clone, Serialize)]
pub struct ModelPoolFallbackSourceStatus {
    pub provider_id: String,
    pub provider_name: Option<String>,
    pub local_model: String,
    pub upstream_model: String,
    pub protocol: UpstreamProtocol,
    pub ready: bool,
    pub availability: String,
    pub reason: Option<String>,
    pub cooling_until: Option<u64>,
}

/// Nonsecret readiness of one stored fallback gateway model and every provider
/// mapping that currently serves it, in the stored list order.
#[derive(Debug, Clone, Serialize)]
pub struct ModelPoolFallbackModelStatus {
    /// The trimmed gateway local model id stored in the pool.
    pub model: String,
    /// Whether at least one expanded source can serve right now.
    pub ready: bool,
    /// Number of expanded sources that are ready.
    pub ready_sources: usize,
    /// Number of known provider mappings serving the model.
    pub total_sources: usize,
    pub sources: Vec<ModelPoolFallbackSourceStatus>,
    /// Bounded nonsecret explanation when no source is ready.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Nonsecret readiness of one stored pool for the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ModelPoolStatus {
    pub id: String,
    pub name: String,
    pub model: String,
    pub enabled: bool,
    /// Canonical pool default token (`auto` or a preference).
    pub default_reasoning_effort: String,
    /// Supported canonical preferences in canonical order.
    pub reasoning_efforts: Vec<String>,
    /// The five canonical coverage rows.
    pub reasoning_coverage: Vec<ModelPoolReasoningCoverage>,
    /// Regular candidate readiness; always regular-only.
    pub candidates: Vec<ModelPoolCandidateStatus>,
    /// Ordered per-model readiness of the stored fallback list. Empty when the
    /// pool configures no fallback model.
    pub fallback_models: Vec<ModelPoolFallbackModelStatus>,
}

/// Nonsecret status of one stored candidate reference. The regular and fallback
/// roles share this shape so their identity and availability stay comparable.
fn candidate_status(
    config: &GatewayConfig,
    stored: &ModelPoolCandidate,
    resolved: &ResolvedCandidate,
) -> ModelPoolCandidateStatus {
    let provider_name = config
        .providers
        .iter()
        .find(|provider| provider.id == resolved.identity.provider_id)
        .map(|provider| provider.name.clone());
    ModelPoolCandidateStatus {
        provider_id: stored.provider_id.trim().to_string(),
        provider_name,
        local_model: stored.local_model.trim().to_string(),
        upstream_model: stored.upstream_model.trim().to_string(),
        protocol: stored.protocol,
        weight: stored.weight,
        ready: resolved.available,
        availability: resolved.availability.to_string(),
        reason: resolved.reason.clone(),
        cooling_until: resolved.cooling_until,
    }
}

/// Nonsecret status of one expanded fallback source mapping.
fn fallback_source_status(
    config: &GatewayConfig,
    stored: &ModelPoolCandidate,
    resolved: &ResolvedCandidate,
) -> ModelPoolFallbackSourceStatus {
    let provider_name = config
        .providers
        .iter()
        .find(|provider| provider.id == resolved.identity.provider_id)
        .map(|provider| provider.name.clone());
    ModelPoolFallbackSourceStatus {
        provider_id: stored.provider_id.trim().to_string(),
        provider_name,
        local_model: stored.local_model.trim().to_string(),
        upstream_model: stored.upstream_model.trim().to_string(),
        protocol: stored.protocol,
        ready: resolved.available,
        availability: resolved.availability.to_string(),
        reason: resolved.reason.clone(),
        cooling_until: resolved.cooling_until,
    }
}

/// Build one fallback model's status record from every provider mapping that
/// serves it, independent of any request ingress.
fn fallback_model_status(
    config: &GatewayConfig,
    pool: &ModelPool,
    alias: &str,
    model: &str,
    now: u64,
) -> ModelPoolFallbackModelStatus {
    let model = model.trim().to_string();
    let expanded = expand_fallback_model_in(&config.providers, &model);
    let sources: Vec<ModelPoolFallbackSourceStatus> = expanded
        .iter()
        .map(|candidate| {
            let resolved = resolve_candidate(config, pool, alias, candidate, None, None, now);
            fallback_source_status(config, candidate, &resolved)
        })
        .collect();
    let ready_sources = sources.iter().filter(|source| source.ready).count();
    let total_sources = sources.len();
    let reason = if ready_sources > 0 {
        None
    } else if total_sources == 0 {
        Some("no provider mapping serves this fallback model".to_string())
    } else {
        sources
            .iter()
            .find_map(|source| source.reason.clone())
            .or_else(|| Some("no expanded source is currently available".to_string()))
    };
    ModelPoolFallbackModelStatus {
        model,
        ready: ready_sources > 0,
        ready_sources,
        total_sources,
        sources,
        reason,
    }
}

/// Build the nonsecret per-pool/per-candidate status from the current
/// configuration and the process-memory cooldown deadlines. No network or quota
/// probe is issued and nothing is persisted.
pub(in crate::ai_gateway) fn pool_status(config: &GatewayConfig) -> Vec<ModelPoolStatus> {
    let now = now_ts();
    config
        .model_pools
        .iter()
        .map(|pool| {
            let alias = pool.model.trim();
            let resolved = resolve_pool_candidates(config, pool, alias, None, None, now);
            let reasoning_catalog =
                super::reasoning::pool_reasoning_catalog(&config.providers, pool);
            let candidates = pool
                .candidates
                .iter()
                .zip(resolved.iter())
                .map(|(stored, resolved)| candidate_status(config, stored, resolved))
                .collect();
            // Each stored fallback model is resolved independently of any request
            // ingress, so its record reflects only its own expanded readiness.
            let fallback_models = pool
                .fallback_models
                .iter()
                .map(|model| fallback_model_status(config, pool, alias, model, now))
                .collect();
            ModelPoolStatus {
                id: pool.id.clone(),
                name: pool.name.trim().to_string(),
                model: alias.to_string(),
                enabled: pool.enabled,
                default_reasoning_effort: reasoning_catalog.default_reasoning_effort,
                reasoning_efforts: reasoning_catalog.reasoning_efforts,
                reasoning_coverage: reasoning_catalog
                    .reasoning_coverage
                    .into_iter()
                    .map(|row| ModelPoolReasoningCoverage {
                        effort: row.effort,
                        mapped_candidates: row.mapped_candidates,
                        total_candidates: row.total_candidates,
                    })
                    .collect(),
                candidates,
                fallback_models,
            }
        })
        .collect()
}
