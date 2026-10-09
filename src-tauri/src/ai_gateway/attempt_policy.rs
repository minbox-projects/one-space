//! Shared per-request candidate/key/retry/probe decision core (REQ-007/AC-007).
//!
//! This module owns the transport-independent decision logic that the buffered
//! [`super::runtime_http::attempt_non_streaming`] and streaming
//! [`super::runtime_http::attempt_streaming`] transports both drive: retry
//! scheduling and the 120-second wait budget, key selection from the ordered
//! pool with the latest persisted marks, key-domain classification and
//! in-request rotation, mapping-health accumulation and its serialized
//! settlement against the latest on-disk configuration, half-open probe
//! settlement, and all-unavailable composition.
//!
//! Transport-specific handling stays in `runtime_http.rs`/`forwarding.rs`: the
//! first-byte boundary, SSE replay, cancellation semantics and every byte of
//! upstream/downstream I/O. The functions here decide *what* to do next; the
//! transports execute the I/O.

use super::runtime_http::{emit_config_updated, emit_key_auth_failed, emit_mapping_auto_disabled};
use super::selection::{
    clear_key_runtime_state, clear_mapping_runtime_state, find_key_probe_candidate,
    is_authentication_error_message, is_quota_exceeded_message, mapping_matches_key,
    mark_key_failure, quota_mark_expired, rearm_key_probe, rearm_mapping_probe_cooldown,
    register_mapping_failure, register_mapping_success, select_usable_key, FailureClass,
    MappingTarget,
};
use super::storage::{modify_config, read_config};
use super::usage_log::sanitize_error_text;
use super::{now_ts, GatewayConfig, GatewayUpstreamProvider, KeyFailureKind};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::time::{sleep, Instant};

/// Actionable clause appended to the all-unavailable message when at least one
/// reported provider has an authentication-failed excluded key. Authentication
/// marks are manual-only, so the operator must re-enable those keys in the AI
/// Gateway.
const AUTHENTICATION_REENABLE_HINT: &str =
    "re-enable authentication-failed keys manually in the AI Gateway";

/// Compose the caller-facing all-unavailable diagnostic from the per-provider
/// failures recorded during a request, adding the quota, network and
/// authentication hints only when every reported provider qualifies
/// (REQ-007/AC-007 exhaustion composition).
pub(in crate::ai_gateway) fn all_unavailable_message(failures: &[(String, String)]) -> String {
    if failures.is_empty() {
        return "all providers unavailable: every candidate failed".to_string();
    }
    let summary = failures
        .iter()
        .map(|(name, reason)| format!("{name}: {reason}"))
        .collect::<Vec<_>>()
        .join("; ");

    let hint = if failures.iter().all(|(_, r)| {
        r.contains("Quota Exceeded") || (r.contains("429") && r.contains("额度已用尽"))
    }) {
        " [提示: 所有服务商额度均已耗尽，请更换服务商或检查账户额度]"
    } else if failures.iter().all(|(_, r)| r.contains("network error")) {
        " [提示: 无法连接到上游服务，请检查服务商 Base URL 与网络/代理设置]"
    } else {
        ""
    };
    // The authentication hint is appended once, only when an authentication
    // -failed key was actually excluded from a candidate key pool (the summary is
    // the only failure reason that can carry that phrase).
    let auth_hint = if failures.iter().any(|(_, r)| {
        r.starts_with("no usable upstream key (") && r.contains("authentication failed")
    }) {
        format!(" ({AUTHENTICATION_REENABLE_HINT})")
    } else {
        String::new()
    };
    format!("all providers unavailable: {summary}{hint}{auth_hint}")
}

/// One provider whose mapping rows entered `auto_disabled` during a successful
/// settlement, carrying everything the message-center input needs. The joined
/// `models` are the provider's complete auto-disabled set after the mutation, in
/// row order; no key value is ever carried (REQ-005/AC-005).
pub(in crate::ai_gateway) struct MappingAutoDisabledTransition {
    pub(in crate::ai_gateway) provider_id: String,
    pub(in crate::ai_gateway) provider_name: String,
    pub(in crate::ai_gateway) models: String,
}

/// The display names of every auto-disabled mapping row of `provider`, in row
/// order, joined with `, `. A row contributes its trimmed `local_model` when
/// non-empty, otherwise its `upstream_model`.
fn auto_disabled_models(provider: &GatewayUpstreamProvider) -> String {
    provider
        .mappings
        .iter()
        .filter(|mapping| mapping.auto_disabled)
        .map(|mapping| {
            let local = mapping.local_model.trim();
            if local.is_empty() {
                mapping.upstream_model.clone()
            } else {
                local.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Whether any row matching `target`'s trimmed key entered `auto_disabled`
/// (`false`/absent -> `true`) since `before`. A probe re-arm of an already
/// disabled row is not a transition (REQ-005/AC-005).
fn auto_disabled_entered(
    provider: &GatewayUpstreamProvider,
    target: &MappingTarget,
    before: &[bool],
) -> bool {
    provider
        .mappings
        .iter()
        .filter(|mapping| mapping_matches_key(mapping, &target.local_model, &target.upstream_model))
        .map(|mapping| mapping.auto_disabled)
        .zip(before.iter().copied())
        .any(|(after, was)| after && !was)
}

/// Record one affected provider id once, preserving first-seen provider order so
/// repeated targets of the same provider settle into a single input. The
/// transition's full auto-disabled set is built after every mutation has been
/// applied, never mid-loop (REQ-005/AC-005).
fn push_auto_disabled_provider_id(provider_ids: &mut Vec<String>, provider_id: &str) {
    if provider_ids.iter().any(|id| id == provider_id) {
        return;
    }
    provider_ids.push(provider_id.to_string());
}

/// Build one transition per affected provider, in first-seen order, from each
/// provider's complete auto-disabled set in the fully mutated configuration.
fn auto_disabled_transitions(
    latest: &GatewayConfig,
    provider_ids: &[String],
) -> Vec<MappingAutoDisabledTransition> {
    provider_ids
        .iter()
        .filter_map(|id| latest.providers.iter().find(|provider| &provider.id == id))
        .map(|provider| MappingAutoDisabledTransition {
            provider_id: provider.id.clone(),
            provider_name: provider.name.clone(),
            models: auto_disabled_models(provider),
        })
        .collect()
}

/// Persist one immediate-disable outcome (401/403) against the latest on-disk
/// configuration.
///
/// The request-start snapshot is never written back: it may predate providers,
/// keys, prices or toggles saved while this request was in flight (a streaming
/// response or retry backoff can span tens of seconds), and a whole-file
/// rewrite from it would silently discard those concurrent edits. Only the
/// matching mapping rows are touched; a provider deleted mid-request stays
/// deleted. The whole read-modify-write runs under the serialized configuration
/// primitive, and a mutation that changes nothing never rewrites the file.
pub(in crate::ai_gateway) fn apply_failure(
    target: &MappingTarget,
    class: FailureClass,
    reason: &str,
) {
    let at = now_ts();
    // The closure returns the transition only once a row entered
    // `auto_disabled`; the message input is built after the write lock is
    // released, and a failed write yields no transition and no input
    // (REQ-005/AC-005).
    let transition = modify_config(|latest| {
        let Some(stored) = latest
            .providers
            .iter_mut()
            .find(|stored| stored.id == target.provider_id)
        else {
            return Ok((false, None));
        };
        let before_runtime = mapping_runtime_snapshot(stored, target);
        let before_auto_disabled = auto_disabled_snapshot(stored, target);
        register_mapping_failure(stored, target, class, reason, at);
        let changed = mapping_runtime_snapshot(stored, target) != before_runtime;
        let transition = if auto_disabled_entered(stored, target, &before_auto_disabled) {
            Some(MappingAutoDisabledTransition {
                provider_id: stored.id.clone(),
                provider_name: stored.name.clone(),
                models: auto_disabled_models(stored),
            })
        } else {
            None
        };
        Ok((changed, transition))
    });
    let Ok(transition) = transition else {
        return;
    };
    if transition.is_some() {
        emit_config_updated();
    }
    if let Some(transition) = transition {
        emit_mapping_auto_disabled(&transition);
    }
}

/// The persisted runtime-health fields of one mapping row, used to detect an
/// actual state change so a settlement that changes nothing never rewrites the
/// file (REQ-002/AC-009).
#[derive(Clone, PartialEq)]
pub(in crate::ai_gateway) struct MappingRuntimeState {
    pub(in crate::ai_gateway) auto_disabled: bool,
    pub(in crate::ai_gateway) disabled_reason: Option<String>,
    pub(in crate::ai_gateway) disabled_at: Option<u64>,
    pub(in crate::ai_gateway) consecutive_failures: u32,
    pub(in crate::ai_gateway) last_error_at: Option<u64>,
}

/// Snapshot the runtime-health fields of every row matching `target`'s trimmed
/// key, in row order.
pub(in crate::ai_gateway) fn mapping_runtime_snapshot(
    provider: &GatewayUpstreamProvider,
    target: &MappingTarget,
) -> Vec<MappingRuntimeState> {
    provider
        .mappings
        .iter()
        .filter(|mapping| mapping_matches_key(mapping, &target.local_model, &target.upstream_model))
        .map(|mapping| MappingRuntimeState {
            auto_disabled: mapping.auto_disabled,
            disabled_reason: mapping.disabled_reason.clone(),
            disabled_at: mapping.disabled_at,
            consecutive_failures: mapping.consecutive_failures,
            last_error_at: mapping.last_error_at,
        })
        .collect()
}

/// Snapshot the `auto_disabled` flags of the mapping rows matching `target`'s
/// trimmed key, so a settlement can detect a transition in either direction.
pub(in crate::ai_gateway) fn auto_disabled_snapshot(
    provider: &GatewayUpstreamProvider,
    target: &MappingTarget,
) -> Vec<bool> {
    provider
        .mappings
        .iter()
        .filter(|mapping| mapping_matches_key(mapping, &target.local_model, &target.upstream_model))
        .map(|mapping| mapping.auto_disabled)
        .collect()
}

/// Whether any row matching `target` changed `auto_disabled` since `before`.
pub(in crate::ai_gateway) fn auto_disabled_flipped(
    provider: &GatewayUpstreamProvider,
    target: &MappingTarget,
    before: &[bool],
) -> bool {
    provider
        .mappings
        .iter()
        .filter(|mapping| mapping_matches_key(mapping, &target.local_model, &target.upstream_model))
        .map(|mapping| mapping.auto_disabled)
        .zip(before.iter().copied())
        .any(|(after, was)| after != was)
}

/// One provider still eligible for a bounded retry inside the current request.
/// This small scheduling state keeps the request's retry queue ordered by the
/// earliest monotonic deadline; equal deadlines keep the initial candidate order.
pub(in crate::ai_gateway) struct RetryCandidate {
    pub(in crate::ai_gateway) provider: GatewayUpstreamProvider,
    pub(in crate::ai_gateway) model: String,
    /// Upstream passes already made for this provider in this request. One pass
    /// attempts each usable key at most once under in-request rotation.
    pub(in crate::ai_gateway) attempts: u32,
    // None represents a valid header delay beyond the clock's range.
    pub(in crate::ai_gateway) ready_at: Option<Instant>,
    /// Keys already attempted in the current pass, so a bare-429 rotation never
    /// contacts the same key twice within one pass. Cleared at every new pass.
    pub(in crate::ai_gateway) attempted_keys: HashSet<String>,
}

impl RetryCandidate {
    /// Pop the earliest-deadline candidate that is ready within `remaining_wait`,
    /// sleeping until its deadline and debiting the elapsed sleep from the
    /// budget. Returns `None` when no candidate is ready inside the budget.
    pub(in crate::ai_gateway) async fn next_ready(
        candidates: &[RetryCandidate],
        remaining_wait: &mut Duration,
    ) -> Option<usize> {
        let index = Self::earliest(candidates)?;
        let ready_at = candidates[index].ready_at?;
        let now = Instant::now();
        let wait = ready_at.saturating_duration_since(now);
        if wait > *remaining_wait {
            return None;
        }
        if !wait.is_zero() {
            sleep(wait).await;
            *remaining_wait = remaining_wait.saturating_sub(now.elapsed());
        }
        Some(index)
    }

    /// Index of the candidate with the earliest deadline, ties by initial order.
    pub(in crate::ai_gateway) fn earliest(candidates: &[RetryCandidate]) -> Option<usize> {
        candidates
            .iter()
            .enumerate()
            .min_by_key(|(index, candidate)| {
                (candidate.ready_at.is_none(), candidate.ready_at, *index)
            })
            .map(|(index, _)| index)
    }
}

/// Result of a single upstream attempt, classified for the retry loop.
pub(in crate::ai_gateway) enum AttemptResult {
    /// A usable response, returned to the caller unchanged.
    Success(super::runtime_http::HttpResponse),
    /// A client error the caller must see unchanged (400/413/422/...).
    ReturnToClient(super::runtime_http::HttpResponse),
    Failure {
        class: FailureClass,
        retryable: bool,
        /// True for a network/transport failure (send, body read, stream open
        /// or mid-stream read); false for an HTTP-status failure.
        transport: bool,
        reason: String,
        retry_delay: Option<Duration>,
        /// When set, the failure is key-scoped: the attempted key must be
        /// marked and the request continues on the next usable key instead of
        /// registering mapping health or requeuing.
        key_failure: Option<KeyFailureKind>,
        /// True for a bare rate-limit 429: the request rotates to the next
        /// usable key of the same provider inside the pass without persisting a
        /// mark, registering mapping health or consuming the retry budget.
        bare_rotation: bool,
    },
}

/// Per-request mapping-row health accumulation.
///
/// Health is counted in inbound-request units, not upstream attempts: however
/// many times a row is tried, its outcome is applied once when the request ends
/// normally. A final success clears the counter, a 404 / rate-limit 429 alone
/// never counts, a quota-exhausted 429 counts once, and a row that also had a
/// network/5xx failure counts once. Only mapping rows
/// carry health: an attempt served through the provider's `default_model` has no
/// target and records nothing.
#[derive(Default)]
pub(in crate::ai_gateway) struct RequestHealth {
    order: Vec<MappingTarget>,
    outcomes: HashMap<MappingTarget, ProviderOutcome>,
    /// Set once per inbound request from the system-resume grace: while true a
    /// transport `Retryable` failure is not counted toward mapping health.
    pub(in crate::ai_gateway) suppress_transport_failures: bool,
    /// The one half-open probe this request attempted, if any. Settled in
    /// [`RequestHealth::apply`] against the latest on-disk configuration.
    probe: Option<ProbeSettlement>,
}

#[derive(Default)]
struct ProviderOutcome {
    health_failure: bool,
    disable_immediately: bool,
    succeeded: bool,
    reason: String,
}

/// Outcome of one attempted half-open probe, buffered until the request ends.
struct ProbeSettlement {
    target: MappingTarget,
    /// Attempt time used to re-arm the cooldown on failure.
    at: u64,
    result: ProbeResult,
}

enum ProbeResult {
    Succeeded,
    Failed { transport: bool, reason: String },
}

impl RequestHealth {
    /// Build a request-scoped accumulator with the resume-grace suppression flag.
    pub(in crate::ai_gateway) fn new(suppress_transport_failures: bool) -> Self {
        Self {
            suppress_transport_failures,
            ..Default::default()
        }
    }

    fn entry(&mut self, target: &MappingTarget) -> &mut ProviderOutcome {
        if !self.outcomes.contains_key(target) {
            self.order.push(target.clone());
            self.outcomes
                .insert(target.clone(), ProviderOutcome::default());
        }
        self.outcomes
            .get_mut(target)
            .expect("health entry inserted above")
    }

    pub(in crate::ai_gateway) fn record_failure(
        &mut self,
        target: &MappingTarget,
        class: FailureClass,
        reason: &str,
        transport: bool,
    ) {
        // A transport failure whose class is Retryable, settled inside the
        // post-resume grace, neither counts nor stamps `last_error_at`
        // (REQ-004/AC-006). HTTP-status failures, immediate auth disables and
        // every other class keep today's behavior.
        if transport && self.suppress_transport_failures && class == FailureClass::Retryable {
            return;
        }
        let entry = self.entry(target);
        match class {
            FailureClass::DisableImmediately => {
                if !entry.disable_immediately {
                    apply_failure(target, class, reason);
                }
                entry.disable_immediately = true;
                entry.reason = reason.to_string();
            }
            FailureClass::Retryable => {
                entry.health_failure = true;
                if entry.reason.is_empty() {
                    entry.reason = reason.to_string();
                }
            }
            // 404 / rate-limit 429 alone never count toward health; other 4xx
            // are returned to the caller and also do not count.
            // Quota-exhausted 429s arrive as Retryable and count above.
            FailureClass::Transient | FailureClass::ReturnToClient => {}
        }
    }

    pub(in crate::ai_gateway) fn record_success(&mut self, target: &MappingTarget) {
        self.entry(target).succeeded = true;
    }

    /// Mark the request's half-open probe as served: the settlement clears the
    /// probed row's runtime state.
    pub(in crate::ai_gateway) fn record_probe_success(&mut self, target: &MappingTarget) {
        self.probe = Some(ProbeSettlement {
            target: target.clone(),
            at: now_ts(),
            result: ProbeResult::Succeeded,
        });
    }

    /// Mark the request's half-open probe as failed at `at`: the settlement
    /// re-arms the probed row's cooldown. A suppressed transport failure keeps
    /// the counter, `last_error_at` and `disabled_reason` untouched.
    pub(in crate::ai_gateway) fn record_probe_failure(
        &mut self,
        target: &MappingTarget,
        at: u64,
        transport: bool,
        reason: &str,
    ) {
        self.probe = Some(ProbeSettlement {
            target: target.clone(),
            at,
            result: ProbeResult::Failed {
                transport,
                reason: reason.to_string(),
            },
        });
    }

    /// Merge this request's probe and non-probe outcomes into the latest on-disk
    /// configuration and persist it. Like [`apply_failure`], this never writes
    /// back a request-start snapshot, so concurrent provider/key/price/toggle
    /// edits survive the settlement of an older in-flight request. The whole
    /// read-modify-write runs under the serialized configuration primitive, and
    /// a settlement that changes no field never rewrites the file
    /// (REQ-002/AC-009).
    pub(in crate::ai_gateway) fn apply(&self) {
        let at = now_ts();
        // Whether any settled row flipped `auto_disabled` during this write:
        // exactly one transition event is emitted after a successful write even
        // when several rows flipped (REQ-005/AC-008).
        let outcome = modify_config(|latest| {
            let mut changed = false;
            let mut flipped = false;
            // One transition per affected provider, preserving first-seen
            // provider order; repeated targets of the same provider collapse
            // into one entry listing its full auto-disabled set (REQ-005/AC-005).
            let mut transition_provider_ids: Vec<String> = Vec::new();
            // The half-open probe settles first: a success clears the row, a
            // failure re-arms its cooldown without ever touching the counter.
            // Details are refreshed unless the failure was a transport failure
            // suppressed by the resume grace (REQ-004/AC-006).
            if let Some(probe) = &self.probe {
                if let Some(stored) = latest
                    .providers
                    .iter_mut()
                    .find(|stored| stored.id == probe.target.provider_id)
                {
                    let before_runtime = mapping_runtime_snapshot(stored, &probe.target);
                    let before_auto_disabled = auto_disabled_snapshot(stored, &probe.target);
                    match &probe.result {
                        ProbeResult::Succeeded => {
                            for mapping in stored.mappings.iter_mut().filter(|mapping| {
                                mapping_matches_key(
                                    mapping,
                                    &probe.target.local_model,
                                    &probe.target.upstream_model,
                                )
                            }) {
                                clear_mapping_runtime_state(mapping);
                            }
                        }
                        ProbeResult::Failed { transport, reason } => {
                            let update_details = !(*transport && self.suppress_transport_failures);
                            let matched = stored.mappings.iter().any(|mapping| {
                                mapping_matches_key(
                                    mapping,
                                    &probe.target.local_model,
                                    &probe.target.upstream_model,
                                )
                            });
                            if matched {
                                rearm_mapping_probe_cooldown(
                                    stored,
                                    &probe.target,
                                    reason,
                                    probe.at,
                                    update_details,
                                );
                            }
                        }
                    }
                    if mapping_runtime_snapshot(stored, &probe.target) != before_runtime {
                        changed = true;
                    }
                    if auto_disabled_flipped(stored, &probe.target, &before_auto_disabled) {
                        flipped = true;
                    }
                    if auto_disabled_entered(stored, &probe.target, &before_auto_disabled) {
                        push_auto_disabled_provider_id(&mut transition_provider_ids, &stored.id);
                    }
                }
            }
            for target in &self.order {
                let Some(outcome) = self.outcomes.get(target) else {
                    continue;
                };
                if outcome.disable_immediately {
                    continue;
                }
                let Some(stored) = latest
                    .providers
                    .iter_mut()
                    .find(|stored| stored.id == target.provider_id)
                else {
                    continue;
                };
                let before_runtime = mapping_runtime_snapshot(stored, target);
                let before_auto_disabled = auto_disabled_snapshot(stored, target);
                if outcome.succeeded {
                    register_mapping_success(stored, target);
                } else if outcome.health_failure {
                    register_mapping_failure(
                        stored,
                        target,
                        FailureClass::Retryable,
                        &outcome.reason,
                        at,
                    );
                } else {
                    continue;
                }
                if mapping_runtime_snapshot(stored, target) != before_runtime {
                    changed = true;
                }
                if auto_disabled_flipped(stored, target, &before_auto_disabled) {
                    flipped = true;
                }
                if auto_disabled_entered(stored, target, &before_auto_disabled) {
                    push_auto_disabled_provider_id(&mut transition_provider_ids, &stored.id);
                }
            }
            // Every mutation has settled; now read each affected provider's
            // complete auto-disabled set from the fully mutated configuration.
            let transitions = auto_disabled_transitions(latest, &transition_provider_ids);
            Ok((changed, (flipped, transitions)))
        });
        // A failed write yields no outcome: nothing is emitted and no input is
        // built. A successful write emits the single config-update event first,
        // then one message input per affected provider, all after the write lock
        // is released (REQ-005/AC-005, AC-008).
        let Ok((flipped, transitions)) = outcome else {
            return;
        };
        if flipped {
            emit_config_updated();
        }
        for transition in &transitions {
            emit_mapping_auto_disabled(transition);
        }
    }
}

/// Settle one finished attempt on the mapping row it belongs to, if any.
///
/// A `default_model` attempt (or any attempt resolving to no matching row)
/// produces no target and therefore no health outcome.
pub(in crate::ai_gateway) fn settle_failure(
    health: &mut RequestHealth,
    provider: &GatewayUpstreamProvider,
    requested: Option<&str>,
    upstream_model: &str,
    class: FailureClass,
    reason: &str,
    transport: bool,
) {
    if let Some(target) = MappingTarget::for_request(provider, requested, upstream_model) {
        health.record_failure(&target, class, reason, transport);
    }
}

/// Settle a served attempt on the mapping row it belongs to, if any.
pub(in crate::ai_gateway) fn settle_success(
    health: &mut RequestHealth,
    provider: &GatewayUpstreamProvider,
    requested: Option<&str>,
    upstream_model: &str,
) {
    if let Some(target) = MappingTarget::for_request(provider, requested, upstream_model) {
        health.record_success(&target);
    }
}

/// One key chosen for an upstream attempt.
pub(in crate::ai_gateway) struct SelectedKey {
    pub(in crate::ai_gateway) id: String,
    pub(in crate::ai_gateway) value: String,
    /// True when this key is a half-open quota probe rather than a normal
    /// selection; a probe is never rotated and re-arms on failure.
    pub(in crate::ai_gateway) probe: bool,
    /// True when this key was selected through a TTL-expired quota mark, so its
    /// runtime state must be cleared in memory and persisted.
    pub(in crate::ai_gateway) ttl_cleared: bool,
}

/// Choose the key for one upstream attempt in list order: the first enabled
/// usable key not yet attempted in this pass (an unmarked key, or a
/// TTL-expired quota-marked key); else, only when no such key remains, at most
/// one eligible quota probe per request; else no key at all. A provider with an
/// empty pool has no usable key and no probe candidate, so it is never
/// attempted and the surrounding loop skips it to the next candidate (REQ-008).
pub(in crate::ai_gateway) fn select_attempt_key(
    provider: &GatewayUpstreamProvider,
    now: u64,
    probe_used: bool,
    attempted: &HashSet<String>,
) -> Option<SelectedKey> {
    if let Some(key) = select_usable_key(provider, now, attempted) {
        return Some(SelectedKey {
            id: key.id.clone(),
            value: key.value.clone(),
            probe: false,
            ttl_cleared: quota_mark_expired(key, now),
        });
    }
    if !probe_used {
        if let Some(key) = find_key_probe_candidate(provider, now) {
            return Some(SelectedKey {
                id: key.id.clone(),
                value: key.value.clone(),
                probe: true,
                ttl_cleared: false,
            });
        }
    }
    None
}

/// Clear a TTL-expired key's runtime state in memory and persist it through the
/// serialized configuration write helper.
pub(in crate::ai_gateway) fn clear_ttl_expired_key_runtime_state(
    provider: &mut GatewayUpstreamProvider,
    selected: &SelectedKey,
) {
    if let Some(key) = provider.keys.iter_mut().find(|key| key.id == selected.id) {
        clear_key_runtime_state(key);
    }
    persist_key_runtime_state(&provider.id, &selected.id, clear_key_runtime_state);
}

/// Classify a pre-first-byte HTTP failure as key-scoped: a 401 always marks the
/// key authentication-failed, a 403 marks it only when its sanitized text names
/// a credential problem, and a quota-classified 400, 402 or 429 marks it
/// quota-exhausted. Every other status is not key-scoped, so a non-credential
/// 403 falls through to the mapping-scoped immediate disable.
pub(in crate::ai_gateway) fn key_failure_kind(
    status: u16,
    error_message: Option<&str>,
) -> Option<KeyFailureKind> {
    match status {
        401 => Some(KeyFailureKind::Authentication),
        403 if is_authentication_error_message(error_message) => {
            Some(KeyFailureKind::Authentication)
        }
        400 | 402 | 429 if is_quota_exceeded_message(error_message) => Some(KeyFailureKind::Quota),
        _ => None,
    }
}

/// Redact every non-empty key value of the pool from one upstream error text.
pub(in crate::ai_gateway) fn sanitize_provider_error_text(
    text: &str,
    provider: &GatewayUpstreamProvider,
) -> Option<String> {
    let mut current = Some(text.to_string());
    for key in &provider.keys {
        if key.value.trim().is_empty() {
            continue;
        }
        current = current.and_then(|value| sanitize_error_text(&value, &key.value));
    }
    current
}

/// Copy the persisted runtime marks of a provider's keys into `provider` by key
/// id, so a request handed a stale snapshot still honors a mark written by an
/// earlier request or attempt. Keys absent from the persisted record keep the
/// snapshot's state; user intent (`enabled`) and values are never copied.
///
/// The configuration read goes through the read cache, so a mark persisted by
/// an earlier attempt in this same request is visible to the next attempt
/// instead of freezing the request-start marks for the whole request.
pub(in crate::ai_gateway) fn sync_key_runtime_marks(provider: &mut GatewayUpstreamProvider) {
    let Ok(latest) = read_config() else {
        return;
    };
    let Some(stored) = latest
        .providers
        .iter()
        .find(|stored| stored.id == provider.id)
    else {
        return;
    };
    for key in &mut provider.keys {
        if let Some(stored_key) = stored.keys.iter().find(|stored| stored.id == key.id) {
            key.auto_marked = stored_key.auto_marked;
            key.failure_kind = stored_key.failure_kind;
            key.marked_at = stored_key.marked_at;
            key.reason = stored_key.reason.clone();
        }
    }
}

/// A key that just transitioned from a non-authentication mark (or no mark)
/// into `Some(KeyFailureKind::Authentication)`, carrying everything the
/// notification needs. The key value is deliberately absent (REQ-005).
pub(in crate::ai_gateway) struct KeyAuthFailedTransition {
    pub(in crate::ai_gateway) provider_id: String,
    pub(in crate::ai_gateway) provider_name: String,
    pub(in crate::ai_gateway) key_id: String,
    pub(in crate::ai_gateway) key_name: String,
    pub(in crate::ai_gateway) reason: String,
    pub(in crate::ai_gateway) marked_at: u64,
}

/// Outcome of one persisted key runtime-state mutation, returned through
/// [`modify_config`] so the emissions happen after the write lock is released.
#[derive(Default)]
struct PersistedKeyOutcome {
    /// Whether the mutation actually changed the persisted key record.
    changed: bool,
    /// The authentication transition the mutation produced, if any.
    transition: Option<KeyAuthFailedTransition>,
}

/// Apply `mutate` to one provider's key in the latest persisted configuration
/// and write it back through the serialized primitive, preserving concurrent
/// edits to every other field. A mutation that leaves the key unchanged does
/// not rewrite the file and emits nothing; a changed mutation emits the
/// config-update event once, and a transition into the authentication mark
/// additionally emits the authentication notification and message input. Both
/// emissions happen after [`modify_config`] returns, so the config write lock is
/// never held while emitting (REQ-004/REQ-005).
pub(in crate::ai_gateway) fn persist_key_runtime_state<F>(
    provider_id: &str,
    key_id: &str,
    mutate: F,
) where
    F: FnOnce(&mut super::UpstreamKey),
{
    if key_id.is_empty() {
        return;
    }
    let outcome = modify_config(|latest| {
        let Some(provider) = latest
            .providers
            .iter_mut()
            .find(|provider| provider.id == provider_id)
        else {
            return Ok((false, PersistedKeyOutcome::default()));
        };
        let stored_provider_id = provider.id.clone();
        let provider_name = provider.name.clone();
        let Some(key) = provider.keys.iter_mut().find(|key| key.id == key_id) else {
            return Ok((false, PersistedKeyOutcome::default()));
        };
        let before = key.clone();
        let was_authentication = key.failure_kind == Some(KeyFailureKind::Authentication);
        mutate(key);
        let changed = *key != before;
        let transition =
            if !was_authentication && key.failure_kind == Some(KeyFailureKind::Authentication) {
                Some(KeyAuthFailedTransition {
                    provider_id: stored_provider_id,
                    provider_name,
                    key_id: key.id.clone(),
                    key_name: key.name.clone(),
                    reason: key.reason.clone().unwrap_or_default(),
                    marked_at: key.marked_at.unwrap_or_else(now_ts),
                })
            } else {
                None
            };
        let outcome = PersistedKeyOutcome {
            changed,
            transition,
        };
        Ok((changed, outcome))
    });

    let Ok(outcome) = outcome else {
        return;
    };
    if outcome.changed {
        emit_config_updated();
    }
    if let Some(transition) = outcome.transition {
        emit_key_auth_failed(&transition);
    }
}

/// Mark a non-probe key after a key-scoped failure and persist it.
pub(in crate::ai_gateway) fn settle_key_failure(
    provider: &mut GatewayUpstreamProvider,
    selected: &SelectedKey,
    kind: KeyFailureKind,
    reason: &str,
    at: u64,
) {
    if let Some(key) = provider.keys.iter_mut().find(|key| key.id == selected.id) {
        mark_key_failure(key, kind, reason, at);
    }
    persist_key_runtime_state(&provider.id, &selected.id, |key| {
        mark_key_failure(key, kind, reason, at);
    });
}

/// Re-arm a failed probe's cooldown and persist it.
pub(in crate::ai_gateway) fn settle_key_probe_failure(
    provider: &mut GatewayUpstreamProvider,
    selected: &SelectedKey,
    kind: Option<KeyFailureKind>,
    reason: &str,
    at: u64,
) {
    if let Some(key) = provider.keys.iter_mut().find(|key| key.id == selected.id) {
        rearm_key_probe(key, kind, reason, at);
    }
    persist_key_runtime_state(&provider.id, &selected.id, |key| {
        rearm_key_probe(key, kind, reason, at);
    });
}

/// Clear a successful probe's runtime state and persist it.
pub(in crate::ai_gateway) fn settle_key_probe_success(
    provider: &mut GatewayUpstreamProvider,
    selected: &SelectedKey,
) {
    if let Some(key) = provider.keys.iter_mut().find(|key| key.id == selected.id) {
        clear_key_runtime_state(key);
    }
    persist_key_runtime_state(&provider.id, &selected.id, clear_key_runtime_state);
}

/// Keep one failure entry per provider in the all-unavailable message, holding
/// the most recent reason, so bounded retries do not spam the same provider.
pub(in crate::ai_gateway) fn record_provider_failure(
    failures: &mut Vec<(String, String)>,
    name: &str,
    reason: String,
) {
    if let Some(entry) = failures.iter_mut().find(|(provider, _)| provider == name) {
        entry.1 = reason;
    } else {
        failures.push((name.to_string(), reason));
    }
}

/// Record a provider failure only when the provider has no entry yet. A provider
/// already exhausted by a key-scoped failure keeps that real upstream reason
/// instead of being replaced by a generic "no usable key" message.
pub(in crate::ai_gateway) fn record_provider_failure_if_absent(
    failures: &mut Vec<(String, String)>,
    name: &str,
    reason: &str,
) {
    if !failures.iter().any(|(provider, _)| provider == name) {
        failures.push((name.to_string(), reason.to_string()));
    }
}

/// The actionable reason for a provider that has no usable key, built from its
/// key pool in the fixed category order authentication failed, quota exhausted,
/// user disabled and listing only non-zero categories.
///
/// A key with `enabled == false` counts as user disabled regardless of any stale
/// runtime mark; an enabled key counts by its runtime mark. `request_has_enabled_key`
/// is the request-level fact that at least one candidate provider still has an
/// enabled key: when the whole request has none, the reason collapses to the
/// request-wide `no enabled key` wording instead of per-key counts (AC-006).
/// The summary never includes a key value.
pub(in crate::ai_gateway) fn excluded_key_summary(
    provider: &GatewayUpstreamProvider,
    request_has_enabled_key: bool,
) -> String {
    let mut authentication = 0usize;
    let mut quota = 0usize;
    let mut user_disabled = 0usize;
    for key in &provider.keys {
        if !key.enabled {
            user_disabled += 1;
            continue;
        }
        match key.failure_kind {
            Some(KeyFailureKind::Authentication) => authentication += 1,
            Some(KeyFailureKind::Quota) => quota += 1,
            None => {}
        }
    }
    if authentication == 0 && quota == 0 && user_disabled == 0 {
        return "no usable upstream key (no enabled key)".to_string();
    }
    if !request_has_enabled_key {
        return "no usable upstream key (no enabled key)".to_string();
    }
    let mut categories = Vec::new();
    if authentication > 0 {
        categories.push(format!("{authentication} authentication failed"));
    }
    if quota > 0 {
        categories.push(format!("{quota} quota exhausted"));
    }
    if user_disabled > 0 {
        categories.push(format!("{user_disabled} user disabled"));
    }
    format!("no usable upstream key ({})", categories.join(", "))
}
