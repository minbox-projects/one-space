use super::candidate::expand_fallback_model_in;
use super::scheduler::{candidate_key, candidate_key_of, pool_cooldown, PoolCandidateKey};
use super::super::selection::is_quota_exceeded_message;
use super::super::{
    now_ts, GatewayUpstreamProvider, ModelPool, PoolCandidateIdentity, UpstreamProtocol,
};
use std::collections::HashSet;
use std::time::Duration;
use tokio::time::Instant;

/// Candidate-scoped cooldown applied when an upstream names no delay, and the
/// maximum for any longer header delay. A model-scoped failure therefore never
/// excludes a candidate for more than this bound, while zero and shorter
/// delays keep their exact duration.
pub(in crate::ai_gateway) const POOL_CANDIDATE_COOLDOWN_SECS: u64 = 60;

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
/// wins up to the [`POOL_CANDIDATE_COOLDOWN_SECS`] bound and an absent/invalid
/// delay falls back to that same bound. Zero, shorter and subsecond durations
/// are preserved exactly, so a `retry-after-ms: 0` yields no cooldown at all; a
/// longer upstream delay is capped so a model-scoped failure cannot exclude the
/// candidate until an unrelated reset boundary. Process memory only; nothing is
/// persisted.
pub(in crate::ai_gateway) fn cool_pool_candidate(
    identity: &PoolCandidateIdentity,
    delay: Option<Duration>,
) {
    let max_delay = Duration::from_secs(POOL_CANDIDATE_COOLDOWN_SECS);
    let delay = delay.unwrap_or(max_delay).min(max_delay);
    let deadline = Instant::now() + delay;
    pool_cooldown()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(candidate_key(&identity.pool_id, identity), deadline);
}

/// Release the candidate-scoped cooldown of the exact source that just served a
/// request. Matching spans every pool and every ownership role (regular, key
/// probe and fallback) because one source shared by several pools recovers
/// together, while the original provider, trimmed local/upstream mapping pair
/// and effective upstream protocol must all match. A sibling model, another
/// protocol, a provider-wide clear, a failed or cancelled request and persisted
/// key/mapping marks are therefore untouched. Process memory only; nothing is
/// persisted.
pub(in crate::ai_gateway) fn clear_pool_candidate_cooldowns(
    provider_id: &str,
    local_model: &str,
    upstream_model: &str,
    protocol: UpstreamProtocol,
) {
    let provider_id = provider_id.trim();
    let local_model = local_model.trim();
    let upstream_model = upstream_model.trim();
    let protocol = protocol.endpoint_path();
    pool_cooldown()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .retain(|key, _| {
            key.provider_id != provider_id
                || key.local_model != local_model
                || key.upstream_model != upstream_model
                || key.protocol != protocol
        });
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
    let published = super::super::storage::read_config_shared().ok();
    let owned: Option<HashSet<PoolCandidateKey>> = published.as_ref().map(|config| {
        config
            .model_pools
            .iter()
            .flat_map(|pool| owned_candidate_keys(pool, &config.providers))
            .collect()
    });
    {
        let mut scheduler = super::scheduler::pool_scheduler()
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
