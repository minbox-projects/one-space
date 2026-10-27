#[cfg(test)]
use rand::seq::SliceRandom;
use super::super::{GatewayUpstreamProvider, UpstreamProtocol};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

/// Uniformly shuffle a copy of the candidate list for one request attempt pass.
/// Test-only: production candidate ordering goes through [`weighted_candidates`].
#[cfg(test)]
pub(in crate::ai_gateway) fn shuffled_candidates(
    candidates: &[GatewayUpstreamProvider],
) -> Vec<GatewayUpstreamProvider> {
    let mut ordered = candidates.to_vec();
    ordered.shuffle(&mut rand::thread_rng());
    ordered
}

/// Composite accounting key of the weighted scheduler.
///
/// One entry exists per provider id plus trimmed requested local model plus
/// inbound protocol, so a candidate set that mixes models or protocols can
/// never consume another scope's current-weight. The protocol component is the
/// canonical endpoint suffix returned by [`UpstreamProtocol::endpoint_path`] —
/// a stable `&'static str` — because `UpstreamProtocol` is a small `Copy` enum
/// without a `Hash` implementation and lives outside this module.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct WeightedSchedulerKey {
    provider_id: String,
    local_model: String,
    protocol: &'static str,
}

static WEIGHTED_SCHEDULER: OnceLock<Mutex<HashMap<WeightedSchedulerKey, i64>>> = OnceLock::new();

fn weighted_scheduler() -> &'static Mutex<HashMap<WeightedSchedulerKey, i64>> {
    WEIGHTED_SCHEDULER.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Reset the global weighted round-robin scheduler state for tests.
#[cfg(test)]
pub(in crate::ai_gateway) fn reset_weighted_scheduler_for_test() {
    let mut map = weighted_scheduler().lock().unwrap_or_else(|e| e.into_inner());
    map.clear();
}

/// Test-only observation seam (AC-035): the number of weighted-scheduler
/// entries currently tracked for `provider_id` across every model and protocol
/// scope.
#[cfg(test)]
#[allow(dead_code)]
pub(in crate::ai_gateway) fn weighted_scheduler_entry_count(provider_id: &str) -> usize {
    weighted_scheduler()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .keys()
        .filter(|key| key.provider_id == provider_id)
        .count()
}

/// Drop every weighted-scheduler entry whose provider id is in `provider_ids`.
///
/// Callers pass exactly the ids a successful configuration publication removed
/// (`before - after`), never the ids that merely happen to be absent from the
/// writer's configuration. That keeps entries belonging to other
/// configurations sharing the process-global scheduler — for example another
/// test home — untouched. A failed write never reaches this call.
pub(in crate::ai_gateway) fn prune_weighted_scheduler(provider_ids: &HashSet<String>) {
    if provider_ids.is_empty() {
        return;
    }
    let mut map = weighted_scheduler()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    map.retain(|key, _| !provider_ids.contains(&key.provider_id));
}

/// Smooth Weighted Round-Robin (SWRR) candidate scheduling.
///
/// Returns all candidates ordered with the SWRR primary candidate at index 0,
/// followed by remaining candidates sorted descending by updated current_weight
/// (with provider ID ascending as tie-breaker).
///
/// Accounting state is keyed per provider id, trimmed requested local model and
/// inbound `protocol`, so mixed candidate sets across models or protocols do not
/// skew each model's distribution. `local_model` is normalized exactly as model
/// resolution does: trimmed, with a blank or missing value treated as absent.
pub(in crate::ai_gateway) fn weighted_candidates(
    candidates: &[GatewayUpstreamProvider],
    local_model: Option<&str>,
    protocol: UpstreamProtocol,
) -> Vec<GatewayUpstreamProvider> {
    if candidates.is_empty() {
        return Vec::new();
    }
    if candidates.len() == 1 {
        return candidates.to_vec();
    }
    let local_model = local_model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("")
        .to_string();
    let protocol = protocol.endpoint_path();
    let keys: Vec<WeightedSchedulerKey> = candidates
        .iter()
        .map(|c| WeightedSchedulerKey {
            provider_id: c.id.clone(),
            local_model: local_model.clone(),
            protocol,
        })
        .collect();
    let mut map = weighted_scheduler().lock().unwrap_or_else(|e| e.into_inner());
    let total_weight: i64 = candidates.iter().map(|c| c.weight.max(1) as i64).sum();

    // 1. current_weight += effective_weight
    for (c, key) in candidates.iter().zip(keys.iter()) {
        let cw = map.entry(key.clone()).or_insert(0);
        *cw += c.weight.max(1) as i64;
    }

    // 2. 选择 current_weight 最大的候选，平手按 provider.id 升序决胜
    let mut best_idx = 0;
    let mut best_val = (
        map.get(&keys[0]).copied().unwrap_or(0),
        Reverse(&candidates[0].id),
    );
    for (idx, c) in candidates.iter().enumerate().skip(1) {
        let val = (
            map.get(&keys[idx]).copied().unwrap_or(0),
            Reverse(&c.id),
        );
        if val > best_val {
            best_val = val;
            best_idx = idx;
        }
    }

    // 3. 扣减选中者的 total_weight
    if let Some(cw) = map.get_mut(&keys[best_idx]) {
        *cw -= total_weight;
    }

    // 4. 剩余候选按更新后的 current_weight 降序排列（平手按 id 升序）
    let mut remaining: Vec<(usize, &GatewayUpstreamProvider)> = candidates
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx != best_idx)
        .collect();

    remaining.sort_by(|(a_idx, a), (b_idx, b)| {
        let wa = map.get(&keys[*a_idx]).copied().unwrap_or(0);
        let wb = map.get(&keys[*b_idx]).copied().unwrap_or(0);
        wb.cmp(&wa).then_with(|| a.id.cmp(&b.id))
    });

    // 5. 组合主选 + 降序降级序列
    let mut result = Vec::with_capacity(candidates.len());
    result.push(candidates[best_idx].clone());
    for (_, c) in remaining {
        result.push((*c).clone());
    }
    result
}

/// Choose a single candidate uniformly at random (used by coverage-sensitive tests).
#[cfg(test)]
pub(in crate::ai_gateway) fn pick_candidate(
    candidates: &[GatewayUpstreamProvider],
) -> Option<GatewayUpstreamProvider> {
    shuffled_candidates(candidates).into_iter().next()
}
