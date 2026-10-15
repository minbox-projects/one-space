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

fn scheduler_key(
    pool_id: &str,
    identity: &PoolCandidateIdentity,
    ingress: &'static str,
) -> PoolSchedulerKey {
    PoolSchedulerKey {
        candidate: candidate_key(pool_id, identity),
        ingress,
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

/// The plan for one request whose model is a current or retired pool alias.
pub(in crate::ai_gateway) enum PoolPlan {
    /// The pool has at least one usable selected candidate (or an eligible
    /// probe). `ordered` is the pool SWRR order and never contains an ordinary
    /// or unselected source.
    Ready {
        ordered: Vec<GatewayUpstreamProvider>,
        probe: Option<ProbeCandidate>,
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
                        prepared = Some(prepare_provider(provider, alias, mapping, &identity, candidate.weight));
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
    // any scheduler/health mutation. An `auto` preference keeps every candidate
    // and injects no control.
    let preference = super::reasoning::resolve_pool_preference(pool, request.requested);
    let mut natives: Vec<Option<PoolNativeReasoning>> = Vec::with_capacity(pool.candidates.len());
    if let Some(preference) = preference {
        let mut declarations = 0usize;
        let mut valid = 0usize;
        let mut compatible = 0usize;
        let mut cap_budget: Option<u32> = None;
        let mut reasons: Vec<String> = Vec::new();
        for candidate in &pool.candidates {
            match super::reasoning::classify_candidate_reasoning(
                config,
                candidate,
                preference,
                &request.cap,
            ) {
                super::reasoning::CandidateReasoning::Unmapped => natives.push(None),
                super::reasoning::CandidateReasoning::Invalid(reason) => {
                    declarations += 1;
                    reasons.push(reason);
                    natives.push(None);
                }
                super::reasoning::CandidateReasoning::CapIncompatible { budget } => {
                    declarations += 1;
                    valid += 1;
                    cap_budget.get_or_insert(budget);
                    natives.push(None);
                }
                super::reasoning::CandidateReasoning::Control(control) => {
                    declarations += 1;
                    valid += 1;
                    compatible += 1;
                    natives.push(Some(control));
                }
            }
        }
        if declarations == 0 {
            return Some(PoolPlan::Rejected {
                status: 400,
                code: "unsupported_reasoning_effort",
                message: format!(
                    "model pool '{alias}' does not support reasoning effort '{}': no selected candidate declares a mapping for it",
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
            .map(|candidate| {
                Some(PoolNativeReasoning::provider_default(candidate.protocol))
            })
            .collect();
    }
    let resolved = resolve_pool_candidates(
        config,
        pool,
        alias,
        Some(protocol),
        Some(&natives),
        now,
    );
    // Healthy candidates: usable key, enabled provider/mapping, supported
    // protocol, not cooling and able to serve the resolved preference. Only
    // these consume a primary SWRR slot.
    let healthy: Vec<ResolvedCandidate> = resolved
        .iter()
        .filter(|candidate| {
            candidate.available && candidate.prepared.is_some() && candidate.native.is_some()
        })
        .cloned()
        .collect();
    let mut ordered = pool_weighted_order(&healthy, &pool.id, protocol.endpoint_path());
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
            ordered.push(prepared.clone());
        }
    }
    // At most one half-open probe (mapping or, only when no mapping probe is
    // eligible, a selected auto-disabled mapping) is permitted per pool request.
    let probe = pool_probe(&resolved, alias, protocol, now);
    if ordered.is_empty() && probe.is_none() {
        return Some(PoolPlan::Fenced {
            message: pool_unavailable_message(alias, &resolved),
        });
    }
    Some(PoolPlan::Ready { ordered, probe })
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
/// An unavailable candidate is filtered before it can consume a slot. The
/// returned primary candidate is first, followed by the remaining candidates in
/// descending updated current-weight order.
fn pool_weighted_order(
    candidates: &[ResolvedCandidate],
    pool_id: &str,
    ingress: &'static str,
) -> Vec<GatewayUpstreamProvider> {
    if candidates.is_empty() {
        return Vec::new();
    }
    if candidates.len() == 1 {
        return candidates[0].prepared.clone().into_iter().collect();
    }
    let keys: Vec<PoolSchedulerKey> = candidates
        .iter()
        .map(|candidate| scheduler_key(pool_id, &candidate.identity, ingress))
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
        let val = (map.get(key).copied().unwrap_or(0), Reverse(tokens[idx].clone()));
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
        ordered.push(primary);
    }
    for idx in remaining {
        if let Some(prepared) = candidates[idx].prepared.clone() {
            ordered.push(prepared);
        }
    }
    ordered
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

/// Prune pool scheduler and cooldown entries owned by pool/candidate removals
/// after a successful configuration publication. A failed write never reaches
/// this call.
pub(in crate::ai_gateway) fn prune_removed_pools(before: &[ModelPool], after: &[ModelPool]) {
    let mut removed_keys: Vec<PoolCandidateKey> = Vec::new();
    let mut removed_pools: Vec<String> = Vec::new();
    for old in before {
        match after.iter().find(|pool| pool.id == old.id) {
            None => removed_pools.push(old.id.trim().to_string()),
            Some(new) => {
                for candidate in &old.candidates {
                    let kept = new.candidates.iter().any(|other| {
                        other.provider_id.trim() == candidate.provider_id.trim()
                            && other.local_model.trim() == candidate.local_model.trim()
                            && other.upstream_model.trim() == candidate.upstream_model.trim()
                            && other.protocol == candidate.protocol
                    });
                    if !kept {
                        removed_keys.push(candidate_key(
                            &old.id,
                            &PoolCandidateIdentity {
                                pool_id: old.id.clone(),
                                provider_id: candidate.provider_id.trim().to_string(),
                                local_model: candidate.local_model.trim().to_string(),
                                upstream_model: candidate.upstream_model.trim().to_string(),
                                protocol: candidate.protocol,
                                reasoning: None,
                            },
                        ));
                    }
                }
            }
        }
    }
    if removed_pools.is_empty() && removed_keys.is_empty() {
        return;
    }
    {
        let mut scheduler = pool_scheduler()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Prune every ingress variant of a removed pool/candidate.
        scheduler.retain(|key, _| {
            !removed_pools.contains(&key.candidate.pool_id)
                && !removed_keys.contains(&key.candidate)
        });
    }
    {
        let mut cooldown = pool_cooldown()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cooldown.retain(|key, _| {
            !removed_pools.contains(&key.pool_id) && !removed_keys.contains(key)
        });
    }
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
    config.providers.iter().any(|provider| {
        provider.id == candidate.provider_id
            && provider.mappings.iter().any(|mapping| {
                mapping.local_model.trim() == candidate.local_model
                    && mapping.upstream_model.trim() == candidate.upstream_model
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
    let existing = config.model_pools.iter().find(|pool| pool.id == incoming.id);
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

/// Nonsecret readiness of one stored pool for the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ModelPoolStatus {
    pub id: String,
    pub name: String,
    pub model: String,
    pub enabled: bool,
    pub candidates: Vec<ModelPoolCandidateStatus>,
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
            let resolved =
                resolve_pool_candidates(config, pool, alias, None, None, now);
            let candidates = pool
                .candidates
                .iter()
                .zip(resolved.iter())
                .map(|(stored, resolved)| {
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
                })
                .collect();
            ModelPoolStatus {
                id: pool.id.clone(),
                name: pool.name.trim().to_string(),
                model: alias.to_string(),
                enabled: pool.enabled,
                candidates,
            }
        })
        .collect()
}
