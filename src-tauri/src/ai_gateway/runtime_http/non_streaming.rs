use super::*;
use super::attempts::{
    attempt_candidate, attempt_target, attempted_source_key, fallback_stage_key,
    AttemptedSourceKey,
};
use super::errors::all_unavailable_payload_for_path;
use super::http_io::json_response;
use super::routing::protocol_for_path;

/// Try the candidates for a non-streaming request: one immediate fallback-first
/// pass in order, then bounded retries per provider in earliest-deadline order.
///
/// Every completed attempt is appended to `attempts` in completion order
/// (REQ-001); the caller owns the buffer so an attempt still in flight when the
/// request ends is dropped without losing the completed entries.
#[cfg(test)]
pub(in crate::ai_gateway) async fn attempt_non_streaming(
    ordered: &[GatewayUpstreamProvider],
    path: &str,
    body: &[u8],
    requested: Option<&str>,
    client_headers: &HashMap<String, String>,
    suppress_transport_failures: bool,
    probe: Option<&ProbeCandidate>,
    attempts: &mut Vec<AttemptLog>,
) -> HttpResponse {
    attempt_non_streaming_inner(
        ordered,
        path,
        body,
        requested,
        client_headers,
        suppress_transport_failures,
        probe,
        attempts,
        &[],
        &GatewaySettings::default(),
    )
    .await
}

async fn attempt_non_streaming_inner(
    ordered: &[GatewayUpstreamProvider],
    path: &str,
    body: &[u8],
    requested: Option<&str>,
    client_headers: &HashMap<String, String>,
    suppress_transport_failures: bool,
    probe: Option<&ProbeCandidate>,
    attempts: &mut Vec<AttemptLog>,
    fallback: &[PoolFallbackStage],
    settings: &GatewaySettings,
) -> HttpResponse {
    let protocol = protocol_for_path(path);
    let mut failures: Vec<(String, String)> = Vec::new();
    // Whether any candidate provider still has an enabled key. When none does,
    // a provider with no usable key reports the request-wide `no enabled key`
    // reason rather than per-key user-disabled counts.
    let request_has_enabled_key = ordered
        .iter()
        .any(|provider| provider.keys.iter().any(|key| key.enabled));
    let mut health = RequestHealth::new(suppress_transport_failures);
    let mut retries: Vec<RetryCandidate> = Vec::new();
    // Whether this request has already used its single key probe.
    let mut key_probe_used = false;
    // Exact source identities this request has already attempted, whether as a
    // regular candidate, a probe or an earlier fallback source. The fallback
    // queue skips an identity already present here.
    let mut attempted_sources: HashSet<AttemptedSourceKey> = HashSet::new();
    // Last attempted provider/model, reported when every candidate is unavailable.
    let mut last_capture: Option<ForwardCapture> = None;

    // Initial pass: every candidate is tried once, in order, without
    // waiting for any backoff, so a healthy later candidate answers before a
    // retry delay is paid. Within one provider slot, a persistent key-scoped
    // failure marks the attempted key and immediately continues on the next
    // usable key without backoff, retry-budget consumption or mapping-health
    // registration, while a bare rate-limit 429 rotates to the next
    // unattempted usable key in the same pass without persisting any mark.
    for provider_snapshot in ordered {
        let Some((model, _)) = attempt_target(provider_snapshot, requested, protocol) else {
            continue;
        };
        let mut provider = provider_snapshot.clone();
        sync_key_runtime_marks(&mut provider);
        let mut attempted_keys: HashSet<String> = HashSet::new();
        // Retryable failure recorded by the last bare-429 rotation of a pass
        // that then ran out of unattempted keys, so the provider still enters
        // the bounded retry schedule with its budget intact.
        let mut pass_retry: Option<(bool, Option<Duration>)> = None;
        let mut selected_none = false;
        loop {
            let Some(selected) =
                select_attempt_key(&provider, now_ts(), key_probe_used, &attempted_keys)
            else {
                selected_none = true;
                break;
            };
            if selected.probe {
                key_probe_used = true;
            }
            if selected.ttl_cleared {
                clear_ttl_expired_key_runtime_state(&mut provider, &selected);
            }
            provider.attempt_key = selected.value.clone();
            if let Some(key) = attempted_source_key(&provider) {
                attempted_sources.insert(key);
            }
            let upstream_protocol = attempt_target(&provider, requested, protocol)
                .map(|(_, upstream)| upstream)
                .unwrap_or(protocol);
            let (outcome, log) = attempt_candidate(
                &provider,
                path,
                body,
                &model,
                requested,
                upstream_protocol,
                client_headers,
                settings,
            )
            .await;
            attempts.push(log);
            match outcome {
                AttemptResult::Success(response) => {
                    if selected.probe {
                        settle_key_probe_success(&mut provider, &selected);
                    }
                    settle_success(
                        &mut health,
                        &provider,
                        requested,
                        &model,
                        upstream_protocol,
                    );
                    health.apply();
                    return response;
                }
                AttemptResult::ReturnToClient(response) => {
                    health.apply();
                    return response;
                }
                AttemptResult::Failure {
                    class,
                    retryable,
                    transport,
                    reason,
                    retry_delay,
                    key_failure,
                    bare_rotation,
                    pool_scope,
                } => {
                    record_provider_failure(
                        &mut failures,
                        &failure_label(&provider),
                        reason.clone(),
                    );
                    last_capture = Some(ForwardCapture {
                        provider_id: provider.id.clone(),
                        provider_name: provider.name.clone(),
                        upstream_model: model.clone(),
                        ..Default::default()
                    });
                    // Model-pool scope: a model-scoped or
                    // unknown-scope quota-like failure cools only this
                    // candidate and immediately falls through to the next
                    // selected candidate without marking the shared key,
                    // exhausting its keys or counting mapping health. It is
                    // never set for an ordinary attempt, whose ordered key
                    // rotation below is unchanged.
                    if pool_scope {
                        if let Some(identity) = provider.pool_candidate.as_ref() {
                            cool_pool_candidate(identity, retry_delay);
                        }
                        break;
                    }
                    if selected.probe {
                        // A failed quota probe re-arms its cooldown and is never
                        // rotated; the next candidate continues the request.
                        settle_key_probe_failure(
                            &mut provider,
                            &selected,
                            key_failure,
                            &reason,
                            now_ts(),
                        );
                        pass_retry = None;
                        attempted_keys.insert(selected.id.clone());
                        break;
                    }
                    if let Some(kind) = key_failure {
                        settle_key_failure(&mut provider, &selected, kind, &reason, now_ts());
                        pass_retry = None;
                        attempted_keys.insert(selected.id.clone());
                        continue;
                    }
                    if bare_rotation {
                        // Rotate inside the pass; the failure is remembered so
                        // an exhausted pass still enters the retry schedule.
                        pass_retry = Some((retryable, retry_delay));
                        attempted_keys.insert(selected.id.clone());
                        continue;
                    }
                    settle_failure(
                        &mut health,
                        &provider,
                        requested,
                        &model,
                        class,
                        &reason,
                        transport,
                    );
                    pass_retry = None;
                    if retryable {
                        retries.push(RetryCandidate {
                            provider: provider.clone(),
                            model: model.clone(),
                            attempts: 1,
                            ready_at: Instant::now()
                                .checked_add(retry_delay.unwrap_or_else(|| default_retry_delay(1))),
                            attempted_keys: HashSet::new(),
                        });
                    }
                    break;
                }
            }
        }
        // A pass exhausted by bare-429 rotation still hands the failure to the
        // existing bounded retry scheduler with the full budget; a provider
        // that had no usable key at all keeps the existing skip message.
        if let Some((retryable, retry_delay)) = pass_retry.take() {
            if retryable {
                retries.push(RetryCandidate {
                    provider: provider.clone(),
                    model,
                    attempts: 1,
                    ready_at: Instant::now()
                        .checked_add(retry_delay.unwrap_or_else(|| default_retry_delay(1))),
                    attempted_keys: HashSet::new(),
                });
            }
        } else if selected_none {
            let reason = excluded_key_summary(&provider, request_has_enabled_key);
            record_provider_failure_if_absent(&mut failures, &failure_label(&provider), &reason);
            last_capture = Some(ForwardCapture {
                provider_id: provider.id.clone(),
                provider_name: provider.name.clone(),
                upstream_model: model.clone(),
                ..Default::default()
            });
        }
    }

    // Bounded retries: serial, earliest deadline first, ties keep the
    // initial candidate order. A 404 never enters this queue and 401/403 stop the
    // provider; only retryable classes are rescheduled. Each retry is a fresh
    // pass that restarts key selection from the first usable key.
    let mut remaining_wait = Duration::from_secs(settings.request_budget_secs as u64);
    loop {
        let Some(index) = RetryCandidate::next_ready(&retries, &mut remaining_wait).await else {
            break;
        };
        let mut candidate = retries.remove(index);
        let model = candidate.model.clone();
        let mut pass_retry: Option<(bool, Option<Duration>)> = None;
        let mut selected_none = false;
        let mut schedule = false;
        loop {
            sync_key_runtime_marks(&mut candidate.provider);
            let Some(selected) = select_attempt_key(
                &candidate.provider,
                now_ts(),
                key_probe_used,
                &candidate.attempted_keys,
            ) else {
                selected_none = true;
                break;
            };
            if selected.probe {
                key_probe_used = true;
            }
            if selected.ttl_cleared {
                clear_ttl_expired_key_runtime_state(&mut candidate.provider, &selected);
            }
            candidate.provider.attempt_key = selected.value.clone();
            if let Some(key) = attempted_source_key(&candidate.provider) {
                attempted_sources.insert(key);
            }
            let upstream_protocol = attempt_target(&candidate.provider, requested, protocol)
                .map(|(_, upstream)| upstream)
                .unwrap_or(protocol);
            let (outcome, log) = attempt_candidate(
                &candidate.provider,
                path,
                body,
                &model,
                requested,
                upstream_protocol,
                client_headers,
                settings,
            )
            .await;
            attempts.push(log);
            match outcome {
                AttemptResult::Success(response) => {
                    if selected.probe {
                        settle_key_probe_success(&mut candidate.provider, &selected);
                    }
                    settle_success(
                        &mut health,
                        &candidate.provider,
                        requested,
                        &model,
                        upstream_protocol,
                    );
                    health.apply();
                    return response;
                }
                AttemptResult::ReturnToClient(response) => {
                    health.apply();
                    return response;
                }
                AttemptResult::Failure {
                    class,
                    retryable,
                    transport,
                    reason,
                    retry_delay,
                    key_failure,
                    bare_rotation,
                    pool_scope,
                } => {
                    record_provider_failure(
                        &mut failures,
                        &failure_label(&candidate.provider),
                        reason.clone(),
                    );
                    last_capture = Some(ForwardCapture {
                        provider_id: candidate.provider.id.clone(),
                        provider_name: candidate.provider.name.clone(),
                        upstream_model: model.clone(),
                        ..Default::default()
                    });
                    if pool_scope {
                        if let Some(identity) = candidate.provider.pool_candidate.as_ref() {
                            cool_pool_candidate(identity, retry_delay);
                        }
                        pass_retry = None;
                        break;
                    }
                    if selected.probe {
                        settle_key_probe_failure(
                            &mut candidate.provider,
                            &selected,
                            key_failure,
                            &reason,
                            now_ts(),
                        );
                        pass_retry = None;
                        candidate.attempted_keys.insert(selected.id.clone());
                        break;
                    }
                    if let Some(kind) = key_failure {
                        settle_key_failure(
                            &mut candidate.provider,
                            &selected,
                            kind,
                            &reason,
                            now_ts(),
                        );
                        pass_retry = None;
                        candidate.attempted_keys.insert(selected.id.clone());
                        continue;
                    }
                    if bare_rotation {
                        candidate.attempted_keys.insert(selected.id.clone());
                        pass_retry = Some((retryable, retry_delay));
                        continue;
                    }
                    settle_failure(
                        &mut health,
                        &candidate.provider,
                        requested,
                        &model,
                        class,
                        &reason,
                        transport,
                    );
                    pass_retry = None;
                    candidate.attempts += 1;
                    if retryable && candidate.attempts <= settings.max_retries_per_provider {
                        candidate.ready_at = Instant::now().checked_add(
                            retry_delay.unwrap_or_else(|| default_retry_delay_with_base(candidate.attempts, settings.retry_base_delay_ms)),
                        );
                        schedule = true;
                    }
                    break;
                }
            }
        }
        // A pass exhausted by bare-429 rotation still schedules the provider's
        // next retry with its budget intact.
        if let Some((retryable, retry_delay)) = pass_retry.take() {
            candidate.attempts += 1;
            if retryable && candidate.attempts <= settings.max_retries_per_provider {
                candidate.ready_at = Instant::now().checked_add(
                    retry_delay.unwrap_or_else(|| default_retry_delay_with_base(candidate.attempts, settings.retry_base_delay_ms)),
                );
                schedule = true;
            }
        } else if selected_none {
            let reason = excluded_key_summary(&candidate.provider, request_has_enabled_key);
            record_provider_failure_if_absent(
                &mut failures,
                &failure_label(&candidate.provider),
                &reason,
            );
        }
        if schedule {
            candidate.attempted_keys.clear();
            retries.insert(index, candidate);
        }
    }

    // Half-open probe: only after every healthy candidate and its
    // bounded retries have failed, try the single eligible auto-disabled row
    // once. The single-flight guard makes concurrent requests skip a probe that
    // is already in flight, and a probe is never queued for retry or backoff.
    // The guard is held until after the failed probe's settlement below so a
    // concurrent request cannot acquire it during the cooldown re-arm window,
    // matching the streaming path.
    let mut _probe_guard: Option<ProbeGuard> = None;
    if let Some(candidate) = probe {
        if let Some(guard) = try_acquire_probe_guard(&candidate.target) {
            // Hold the single-flight guard for the whole probe, including every
            // in-probe key rotation below.
            _probe_guard = Some(guard);
            let mut provider = candidate.provider.clone();
            sync_key_runtime_marks(&mut provider);
            let model = candidate.upstream_model.as_str();
            let mut last_key_failure: Option<String> = None;
            let mut probe_attempted: HashSet<String> = HashSet::new();
            loop {
                let Some(selected) =
                    select_attempt_key(&provider, now_ts(), key_probe_used, &probe_attempted)
                else {
                    // No usable key remains after consuming the key-scoped
                    // failures: re-arm the probe cooldown through the existing
                    // probe-failure path and fall through to the exhausted path.
                    if let Some(reason) = last_key_failure.take() {
                        health.record_probe_failure(&candidate.target, now_ts(), false, &reason);
                        record_provider_failure(&mut failures, &failure_label(&provider), reason);
                        last_capture = Some(ForwardCapture {
                            provider_id: provider.id.clone(),
                            provider_name: provider.name.clone(),
                            upstream_model: candidate.upstream_model.clone(),
                            ..Default::default()
                        });
                    }
                    break;
                };
                if selected.probe {
                    key_probe_used = true;
                }
                if selected.ttl_cleared {
                    clear_ttl_expired_key_runtime_state(&mut provider, &selected);
                }
                provider.attempt_key = selected.value.clone();
                if let Some(key) = attempted_source_key(&provider) {
                    attempted_sources.insert(key);
                }
                // The probed row is auto-disabled, so the normal served-row
                // resolution skips it; forward under the row's own effective
                // protocol so a converted probe reaches `/v1/messages`.
                let upstream_protocol = candidate.protocol;
                let (outcome, log) = attempt_candidate(
                    &provider,
                    path,
                    body,
                    model,
                    requested,
                    upstream_protocol,
                    client_headers,
                    settings,
                )
                .await;
                attempts.push(log);
                match outcome {
                    AttemptResult::Success(response) => {
                        if selected.probe {
                            settle_key_probe_success(&mut provider, &selected);
                        }
                        health.record_probe_success(&candidate.target);
                        health.apply();
                        return response;
                    }
                    AttemptResult::ReturnToClient(response) => {
                        let status = response.status;
                        let reason = format!("HTTP {status} returned to client");
                        if selected.probe {
                            settle_key_probe_failure(
                                &mut provider,
                                &selected,
                                None,
                                &reason,
                                now_ts(),
                            );
                        }
                        health.record_probe_failure(&candidate.target, now_ts(), false, &reason);
                        health.apply();
                        return response;
                    }
                    AttemptResult::Failure {
                        transport,
                        reason,
                        key_failure,
                        retry_delay,
                        pool_scope,
                        ..
                    } => {
                        if pool_scope {
                            if let Some(identity) = provider.pool_candidate.as_ref() {
                                cool_pool_candidate(identity, retry_delay);
                            }
                            record_provider_failure(
                                &mut failures,
                                &failure_label(&provider),
                                reason,
                            );
                            last_capture = Some(ForwardCapture {
                                provider_id: provider.id.clone(),
                                provider_name: provider.name.clone(),
                                upstream_model: candidate.upstream_model.clone(),
                                ..Default::default()
                            });
                            break;
                        }
                        if selected.probe {
                            // A key selected as the quota probe is never rotated:
                            // its failure re-arms the probe and ends the probe.
                            settle_key_probe_failure(
                                &mut provider,
                                &selected,
                                key_failure,
                                &reason,
                                now_ts(),
                            );
                            health.record_probe_failure(
                                &candidate.target,
                                now_ts(),
                                transport,
                                &reason,
                            );
                            record_provider_failure(
                                &mut failures,
                                &failure_label(&provider),
                                reason,
                            );
                            last_capture = Some(ForwardCapture {
                                provider_id: provider.id.clone(),
                                provider_name: provider.name.clone(),
                                upstream_model: candidate.upstream_model.clone(),
                                ..Default::default()
                            });
                            break;
                        }
                        if let Some(kind) = key_failure {
                            // A normally selected usable key failed auth/quota
                            // inside the probe: mark it and continue the same
                            // probe on the provider's next usable key, without
                            // registering mapping health or consuming the retry
                            // budget.
                            let at = now_ts();
                            settle_key_failure(&mut provider, &selected, kind, &reason, at);
                            probe_attempted.insert(selected.id.clone());
                            last_key_failure = Some(reason);
                            continue;
                        }
                        health.record_probe_failure(
                            &candidate.target,
                            now_ts(),
                            transport,
                            &reason,
                        );
                        record_provider_failure(&mut failures, &failure_label(&provider), reason);
                        last_capture = Some(ForwardCapture {
                            provider_id: provider.id.clone(),
                            provider_name: provider.name.clone(),
                            upstream_model: candidate.upstream_model.clone(),
                            ..Default::default()
                        });
                        // The probe is never requeued; fall through to the existing
                        // exhausted path so the 502 envelope names the provider.
                        break;
                    }
                }
            }
        }
    }

    // Distinct last-stage fallback queue, consumed only after every regular
    // candidate, its finite retries and the regular probe are exhausted. Each
    // queued source receives one provider pass sharing this request's single key
    // probe allowance, and every source already attempted by this request is
    // skipped without consuming the remaining sources of its model. The queue
    // owns no delayed retry cycle.
    for stage in fallback {
        if let Some(key) = fallback_stage_key(stage) {
            if attempted_sources.contains(&key) {
                continue;
            }
        }
        match stage {
            PoolFallbackStage::Ready(provider_snapshot) => {
                let mut provider = provider_snapshot.clone();
                sync_key_runtime_marks(&mut provider);
                if let Some((model, _)) = attempt_target(&provider, requested, protocol) {
                    let upstream_protocol = attempt_target(&provider, requested, protocol)
                        .map(|(_, upstream)| upstream)
                        .unwrap_or(protocol);
                    let mut attempted_keys: HashSet<String> = HashSet::new();
                    loop {
                        let Some(selected) = select_attempt_key(
                            &provider,
                            now_ts(),
                            key_probe_used,
                            &attempted_keys,
                        ) else {
                            let reason = excluded_key_summary(&provider, request_has_enabled_key);
                            record_provider_failure_if_absent(
                                &mut failures,
                                &failure_label(&provider),
                                &reason,
                            );
                            break;
                        };
                        if selected.probe {
                            key_probe_used = true;
                        }
                        if selected.ttl_cleared {
                            clear_ttl_expired_key_runtime_state(&mut provider, &selected);
                        }
                        provider.attempt_key = selected.value.clone();
                        if let Some(key) = attempted_source_key(&provider) {
                            attempted_sources.insert(key);
                        }
                        let (outcome, log) = attempt_candidate(
                            &provider,
                            path,
                            body,
                            &model,
                            requested,
                            upstream_protocol,
                            client_headers,
                            settings,
                        )
                        .await;
                        attempts.push(log);
                        match outcome {
                            AttemptResult::Success(mut response) => {
                                if selected.probe {
                                    settle_key_probe_success(&mut provider, &selected);
                                }
                                settle_success(
                                    &mut health,
                                    &provider,
                                    requested,
                                    &model,
                                    upstream_protocol,
                                );
                                health.apply();
                                if let Some(capture) = response.capture.as_mut() {
                                    capture.fallback_stage = true;
                                }
                                return response;
                            }
                            AttemptResult::ReturnToClient(response) => {
                                health.apply();
                                return response;
                            }
                            AttemptResult::Failure {
                                transport,
                                class,
                                retry_delay,
                                reason,
                                key_failure,
                                bare_rotation,
                                pool_scope,
                                ..
                            } => {
                                record_provider_failure(
                                    &mut failures,
                                    &failure_label(&provider),
                                    reason.clone(),
                                );
                                last_capture = Some(ForwardCapture {
                                    provider_id: provider.id.clone(),
                                    provider_name: provider.name.clone(),
                                    upstream_model: model.clone(),
                                    ..Default::default()
                                });
                                if pool_scope {
                                    if let Some(identity) = provider.pool_candidate.as_ref() {
                                        cool_pool_candidate(identity, retry_delay);
                                    }
                                    break;
                                }
                                if selected.probe {
                                    settle_key_probe_failure(
                                        &mut provider,
                                        &selected,
                                        key_failure,
                                        &reason,
                                        now_ts(),
                                    );
                                    break;
                                }
                                if let Some(kind) = key_failure {
                                    settle_key_failure(
                                        &mut provider,
                                        &selected,
                                        kind,
                                        &reason,
                                        now_ts(),
                                    );
                                    attempted_keys.insert(selected.id.clone());
                                    continue;
                                }
                                if bare_rotation {
                                    attempted_keys.insert(selected.id.clone());
                                    continue;
                                }
                                settle_failure(
                                    &mut health,
                                    &provider,
                                    requested,
                                    &model,
                                    class,
                                    &reason,
                                    transport,
                                );
                                break;
                            }
                        }
                    }
                }
            }
            PoolFallbackStage::Probe(candidate) => {
                if _probe_guard.is_none() {
                    match try_acquire_probe_guard(&candidate.target) {
                        Some(guard) => _probe_guard = Some(guard),
                        None => {
                            record_provider_failure_if_absent(
                                &mut failures,
                                &failure_label(&candidate.provider),
                                "another request is already probing this fallback mapping",
                            );
                        }
                    }
                }
                if _probe_guard.is_some() {
                    let mut provider = candidate.provider.clone();
                    sync_key_runtime_marks(&mut provider);
                    let model = candidate.upstream_model.clone();
                    let mut probe_attempted: HashSet<String> = HashSet::new();
                    let mut last_key_failure: Option<String> = None;
                    loop {
                        let Some(selected) = select_attempt_key(
                            &provider,
                            now_ts(),
                            key_probe_used,
                            &probe_attempted,
                        ) else {
                            if let Some(reason) = last_key_failure.take() {
                                health.record_probe_failure(
                                    &candidate.target,
                                    now_ts(),
                                    false,
                                    &reason,
                                );
                                record_provider_failure(
                                    &mut failures,
                                    &failure_label(&provider),
                                    reason,
                                );
                            }
                            break;
                        };
                        if selected.probe {
                            key_probe_used = true;
                        }
                        if selected.ttl_cleared {
                            clear_ttl_expired_key_runtime_state(&mut provider, &selected);
                        }
                        provider.attempt_key = selected.value.clone();
                        if let Some(key) = attempted_source_key(&provider) {
                            attempted_sources.insert(key);
                        }
                        let (outcome, log) = attempt_candidate(
                            &provider,
                            path,
                            body,
                            &model,
                            requested,
                            candidate.protocol,
                            client_headers,
                            settings,
                        )
                        .await;
                        attempts.push(log);
                        match outcome {
                            AttemptResult::Success(mut response) => {
                                if selected.probe {
                                    settle_key_probe_success(&mut provider, &selected);
                                }
                                health.record_probe_success(&candidate.target);
                                health.apply();
                                if let Some(capture) = response.capture.as_mut() {
                                    capture.fallback_stage = true;
                                }
                                return response;
                            }
                            AttemptResult::ReturnToClient(response) => {
                                let status = response.status;
                                let reason = format!("HTTP {status} returned to client");
                                if selected.probe {
                                    settle_key_probe_failure(
                                        &mut provider,
                                        &selected,
                                        None,
                                        &reason,
                                        now_ts(),
                                    );
                                }
                                health.record_probe_failure(
                                    &candidate.target,
                                    now_ts(),
                                    false,
                                    &reason,
                                );
                                health.apply();
                                return response;
                            }
                            AttemptResult::Failure {
                                transport,
                                reason,
                                key_failure,
                                retry_delay,
                                pool_scope,
                                ..
                            } => {
                                if pool_scope {
                                    if let Some(identity) = provider.pool_candidate.as_ref() {
                                        cool_pool_candidate(identity, retry_delay);
                                    }
                                    record_provider_failure(
                                        &mut failures,
                                        &failure_label(&provider),
                                        reason,
                                    );
                                    break;
                                }
                                if selected.probe {
                                    settle_key_probe_failure(
                                        &mut provider,
                                        &selected,
                                        key_failure,
                                        &reason,
                                        now_ts(),
                                    );
                                    health.record_probe_failure(
                                        &candidate.target,
                                        now_ts(),
                                        transport,
                                        &reason,
                                    );
                                    record_provider_failure(
                                        &mut failures,
                                        &failure_label(&provider),
                                        reason,
                                    );
                                    break;
                                }
                                if let Some(kind) = key_failure {
                                    settle_key_failure(
                                        &mut provider,
                                        &selected,
                                        kind,
                                        &reason,
                                        now_ts(),
                                    );
                                    probe_attempted.insert(selected.id.clone());
                                    last_key_failure = Some(reason);
                                    continue;
                                }
                                health.record_probe_failure(
                                    &candidate.target,
                                    now_ts(),
                                    transport,
                                    &reason,
                                );
                                record_provider_failure(
                                    &mut failures,
                                    &failure_label(&provider),
                                    reason,
                                );
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    health.apply();
    let message = all_unavailable_message(&failures);
    let mut response = json_response(502, all_unavailable_payload_for_path(path, message.clone()));
    let mut capture = last_capture.unwrap_or_default();
    capture.status = 502;
    capture.protocol = protocol;
    capture.all_unavailable = true;
    // The terminal row records the same diagnostic the caller received.
    capture.message = Some(message);
    response.capture = Some(capture);
    response
}

/// Run the non-streaming attempt loop for one inbound request.
pub(super) async fn attempt_non_streaming_request(
    ordered: &[GatewayUpstreamProvider],
    path: &str,
    body: &[u8],
    requested: Option<&str>,
    client_headers: &HashMap<String, String>,
    suppress_transport_failures: bool,
    probe: Option<&ProbeCandidate>,
    attempts: &mut Vec<AttemptLog>,
    fallback: &[PoolFallbackStage],
    settings: &GatewaySettings,
) -> HttpResponse {
    attempt_non_streaming_inner(
        ordered,
        path,
        body,
        requested,
        client_headers,
        suppress_transport_failures,
        probe,
        attempts,
        fallback,
        settings,
    )
    .await
}

