use super::candidate::{resolve_candidate, ResolvedCandidate};
use super::health::pool_candidate_cooling;
use super::planning::PoolFallbackStage;
use super::super::selection::{find_key_probe_candidate, find_probe_candidate, MappingTarget};
use super::super::{
    GatewayConfig, GatewayUpstreamProvider, ModelPool, ModelPoolCandidate, PoolCandidateIdentity,
    PoolNativeReasoning, UpstreamProtocol, MAX_PROVIDER_WEIGHT,
};
use std::cmp::Reverse;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tokio::time::Instant;

/// Composite identity of one pool candidate, used for the candidate-scoped
/// cooldown table. Protocol is stored as the canonical endpoint suffix so it
/// stays a stable, hashable `&'static str`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct PoolCandidateKey {
    pub(super) pool_id: String,
    pub(super) provider_id: String,
    pub(super) local_model: String,
    pub(super) upstream_model: String,
    pub(super) protocol: &'static str,
}

/// Scheduler accounting key: one complete candidate identity plus the *ingress*
/// protocol. Two ingresses route to the same stored candidate under different
/// keys, so each ingress owns an independent cycle; the cooldown below stays
/// keyed by the candidate alone and is therefore shared across ingresses for
/// that candidate.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct PoolSchedulerKey {
    pub(super) candidate: PoolCandidateKey,
    pub(super) ingress: &'static str,
    /// `None` for the primary pool order; the stored fallback model id for the
    /// fallback queue. A fallback source therefore never shares accounting with
    /// the primary order or with another fallback model.
    pub(super) fallback_model: Option<String>,
}

pub(super) fn candidate_key(pool_id: &str, identity: &PoolCandidateIdentity) -> PoolCandidateKey {
    PoolCandidateKey {
        pool_id: pool_id.trim().to_string(),
        provider_id: identity.provider_id.trim().to_string(),
        local_model: identity.local_model.trim().to_string(),
        upstream_model: identity.upstream_model.trim().to_string(),
        protocol: identity.protocol.endpoint_path(),
    }
}

/// Cooldown/scheduler key of one stored candidate reference in a pool.
pub(super) fn candidate_key_of(
    pool_id: &str,
    candidate: &ModelPoolCandidate,
) -> PoolCandidateKey {
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

pub(super) fn pool_scheduler() -> &'static Mutex<HashMap<PoolSchedulerKey, i64>> {
    POOL_SCHEDULER.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn pool_cooldown() -> &'static Mutex<HashMap<PoolCandidateKey, Instant>> {
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
pub(super) fn note_pool_provider_copy() {
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
pub(super) fn pool_weighted_order(
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
pub(super) fn fallback_queue(
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
