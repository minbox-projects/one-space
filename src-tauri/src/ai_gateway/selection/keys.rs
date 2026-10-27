use super::super::{
    GatewayUpstreamProvider, KeyFailureKind, UpstreamKey, KEY_PROBE_COOLDOWN_SECS,
    KEY_QUOTA_MARK_TTL_SECS,
};
use std::collections::HashSet;

/// The fixed UTC+8 natural day index of a UNIX-seconds timestamp.
///
/// The `+08:00` offset is folded in through quotient/remainder decomposition so
/// no timestamp value (including `u64::MAX`) can overflow, and the result is the
/// number of whole UTC+8 calendar days since the UNIX epoch. Comparing two day
/// indices is therefore exactly "the clock's UTC+8 calendar day is later than
/// the other's", which never consults the device timezone.
fn offset_natural_day(timestamp: u64, offset_hours: i32) -> i64 {
    const SECONDS_PER_DAY: i64 = 86_400;
    let offset_seconds = (offset_hours as i64) * 3600;
    ((timestamp as i64) + offset_seconds).div_euclid(SECONDS_PER_DAY)
}

/// Whether a daily-reset quota mark is due at the explicit `now`.
#[allow(dead_code)]
pub(in crate::ai_gateway) fn daily_quota_reset_due(key: &UpstreamKey, now: u64) -> bool {
    daily_quota_reset_due_with_offset(key, now, 8)
}

pub(in crate::ai_gateway) fn daily_quota_reset_due_with_offset(
    key: &UpstreamKey,
    now: u64,
    offset_hours: i32,
) -> bool {
    key.enabled
        && key.daily_reset
        && key.auto_marked
        && key.failure_kind == Some(KeyFailureKind::Quota)
        && key.marked_at.is_some_and(|marked_at| {
            offset_natural_day(now, offset_hours) > offset_natural_day(marked_at, offset_hours)
        })
}

/// Whether a quota mark is old enough to make the key usable again through
/// normal list-order selection.
pub(in crate::ai_gateway) fn quota_mark_expired(key: &UpstreamKey, now: u64) -> bool {
    quota_mark_expired_with_ttl(key, now, KEY_QUOTA_MARK_TTL_SECS, 8)
}

pub(in crate::ai_gateway) fn quota_mark_expired_with_ttl(
    key: &UpstreamKey,
    now: u64,
    ttl_secs: u64,
    offset_hours: i32,
) -> bool {
    if !key.auto_marked || key.failure_kind != Some(KeyFailureKind::Quota) {
        return false;
    }
    if key.daily_reset {
        return daily_quota_reset_due_with_offset(key, now, offset_hours);
    }
    key.marked_at
        .is_some_and(|marked_at| now.saturating_sub(marked_at) >= ttl_secs)
}

pub(in crate::ai_gateway) fn select_usable_key<'a>(
    provider: &'a GatewayUpstreamProvider,
    now: u64,
    attempted: &HashSet<String>,
) -> Option<&'a UpstreamKey> {
    select_usable_key_with_settings(provider, now, attempted, KEY_QUOTA_MARK_TTL_SECS, 8)
}

pub(in crate::ai_gateway) fn select_usable_key_with_settings<'a>(
    provider: &'a GatewayUpstreamProvider,
    now: u64,
    attempted: &HashSet<String>,
    ttl_secs: u64,
    offset_hours: i32,
) -> Option<&'a UpstreamKey> {
    provider.keys.iter().find(|key| {
        key.enabled
            && !attempted.contains(&key.id)
            && (!key.auto_marked || quota_mark_expired_with_ttl(key, now, ttl_secs, offset_hours))
    })
}

pub(in crate::ai_gateway) fn find_key_probe_candidate(
    provider: &GatewayUpstreamProvider,
    now: u64,
) -> Option<&UpstreamKey> {
    find_key_probe_candidate_with_cooldown(provider, now, KEY_PROBE_COOLDOWN_SECS)
}

pub(in crate::ai_gateway) fn find_key_probe_candidate_with_cooldown(
    provider: &GatewayUpstreamProvider,
    now: u64,
    cooldown_secs: u64,
) -> Option<&UpstreamKey> {
    provider
        .keys
        .iter()
        .filter(|key| key.enabled && key.auto_marked && !key.daily_reset)
        .filter(|key| key.failure_kind == Some(KeyFailureKind::Quota))
        .filter(|key| {
            key.marked_at
                .is_some_and(|marked_at| now.saturating_sub(marked_at) >= cooldown_secs)
        })
        .min_by_key(|key| key.marked_at.unwrap_or(u64::MAX))
}

/// Mark a key with a key-scoped failure: it leaves the usable pool and carries
/// the failure kind, marking time and readable reason. The user's `enabled`
/// intent is untouched.
pub(in crate::ai_gateway) fn mark_key_failure(
    key: &mut UpstreamKey,
    kind: KeyFailureKind,
    reason: &str,
    at: u64,
) {
    key.auto_marked = true;
    key.failure_kind = Some(kind);
    key.marked_at = Some(at);
    key.reason = Some(reason.to_string());
}

/// Re-arm a failed probe's cooldown: the key stays runtime-marked and moves its
/// `marked_at` to `at`. An auth failure switches the mark kind to
/// authentication; any other failure keeps the existing quota kind.
pub(in crate::ai_gateway) fn rearm_key_probe(
    key: &mut UpstreamKey,
    kind: Option<KeyFailureKind>,
    reason: &str,
    at: u64,
) {
    key.auto_marked = true;
    match kind {
        Some(kind) => key.failure_kind = Some(kind),
        None => {
            key.failure_kind.get_or_insert(KeyFailureKind::Quota);
        }
    }
    key.marked_at = Some(at);
    key.reason = Some(reason.to_string());
}

/// The key a read-only quota/usage query is pinned to: the first enabled
/// unmarked key in list order, then the first enabled key when every enabled key
/// is marked, then the first stored key when no key is enabled. An empty pool
/// has no pinned key. This source never follows the serving key.
pub(in crate::ai_gateway) fn pinned_key_value(
    provider: &GatewayUpstreamProvider,
) -> Option<&str> {
    provider
        .keys
        .iter()
        .find(|key| key.enabled && !key.auto_marked)
        .or_else(|| provider.keys.iter().find(|key| key.enabled))
        .or_else(|| provider.keys.first())
        .map(|key| key.value.as_str())
}

/// Clear a key's runtime state without touching the user's `enabled` flag.
pub(in crate::ai_gateway) fn clear_key_runtime_state(key: &mut UpstreamKey) {
    key.auto_marked = false;
    key.failure_kind = None;
    key.marked_at = None;
    key.reason = None;
}
