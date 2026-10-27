use super::*;
use super::attempts::{
    attempt_target, attempted_source_key, build_attempt_log, fallback_stage_key,
    is_sse_response_chunk, AttemptedSourceKey,
};
use super::errors::{
    all_unavailable_payload_for_path, classify_attempt_failure, error_envelope,
    extract_upstream_error_type, failure_reason, is_standard_error_body, upstream_error_payload,
};
use super::http_io::{json_response, write_response, write_stream_headers};
use super::routing::protocol_for_path;

/// Try immediate fallback candidates, then bounded retries before downstream output.
///
/// Switching is permitted only until the first byte is written to `writer`; once
/// written, an upstream failure terminates the stream without retrying.
///
/// Every completed attempt is appended to `attempts` in completion order
/// (REQ-001). An attempt whose bytes are still being forwarded when the request
/// ends is in flight and appends nothing.
#[cfg(test)]
pub(in crate::ai_gateway) async fn attempt_streaming<W: AsyncWrite + Unpin>(
    writer: &mut W,
    ordered: &[GatewayUpstreamProvider],
    path: &str,
    body: &[u8],
    requested: Option<&str>,
    client_headers: &HashMap<String, String>,
    suppress_transport_failures: bool,
    probe: Option<&ProbeCandidate>,
    attempts: &mut Vec<AttemptLog>,
) -> Result<ForwardCapture, String> {
    attempt_streaming_inner(
        writer,
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

async fn attempt_streaming_inner<W: AsyncWrite + Unpin>(
    writer: &mut W,
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
) -> Result<ForwardCapture, String> {
    let protocol = protocol_for_path(path);
    let mut failures: Vec<(String, String)> = Vec::new();
    // The caller's own boolean `stream_options.include_usage` (Chat ingress
    // only); a converted Chat stream honors it without injecting the upstream
    // flag the caller never requested.
    let include_usage = serde_json::from_slice::<Value>(body)
        .ok()
        .is_some_and(|value| include_usage_requested(&value));
    // Whether any candidate provider still has an enabled key (AC-006).
    let request_has_enabled_key = ordered
        .iter()
        .any(|provider| provider.keys.iter().any(|key| key.enabled));
    let mut health = RequestHealth::new(suppress_transport_failures);
    let mut retries = Vec::new();
    let mut remaining_wait = Duration::from_secs(settings.request_budget_secs as u64);
    let mut ordered_queue: VecDeque<RetryCandidate> = ordered
        .iter()
        .filter_map(
            |provider| match attempt_target(provider, requested, protocol) {
                Some((model, _)) => Some(RetryCandidate {
                    provider: provider.clone(),
                    model,
                    attempts: 0,
                    ready_at: None,
                    attempted_keys: HashSet::new(),
                }),
                None => None,
            },
        )
        .collect();
    // Whether this request has already used its single key probe.
    let mut key_probe_used = false;
    // Exact source identities this request has already attempted, whether as a
    // regular candidate, a probe or an earlier fallback source. The fallback
    // queue skips an identity already present here.
    let mut attempted_sources: HashSet<AttemptedSourceKey> = HashSet::new();
    // Read-only usage parser fed from the relay's own passthrough loop; it never
    // influences the bytes written to the caller.
    let mut usage = SseUsageAccumulator::default();
    let mut capture = ForwardCapture::default();
    // Real failure status to persist for a terminal all-unavailable outcome: the
    // last upstream HTTP status (0 for a network error). The caller-facing
    // pre-stream failure is a 502 JSON transport, which must never replace that
    // real status in the request log.
    let mut last_failure_status: Option<u16> = None;
    // The optional half-open probe is taken at most once, after every healthy
    // candidate and bounded retry has failed. `_probe_guard` holds the
    // single-flight guard for the whole probe and releases it on return.
    let mut pending_probe: Option<ProbeCandidate> = probe.cloned();
    // The configured fallback queue is consumed only after every regular
    // candidate, its finite retries and the regular probe are exhausted. Once
    // taken, this request is in its fallback pass and never schedules another
    // retry cycle; each queued source receives one provider pass.
    let mut pending_fallback: VecDeque<PoolFallbackStage> =
        fallback.iter().cloned().collect();
    let mut fallback_active = false;
    // Last key-scoped failure reason of an in-probe rotation, used to re-arm the
    // probe cooldown through the existing probe-failure path when no usable key
    // remains.
    let mut probe_key_failure_reason: Option<String> = None;
    let mut _probe_guard: Option<ProbeGuard> = None;
    loop {
        let (mut candidate, retry_index, probe_target, selected, attempt_protocol) =
            if let Some(mut candidate) = ordered_queue.pop_front() {
                sync_key_runtime_marks(&mut candidate.provider);
                let Some(selected) = select_attempt_key(
                    &candidate.provider,
                    now_ts(),
                    key_probe_used,
                    &candidate.attempted_keys,
                ) else {
                    // A provider with no usable key and no eligible probe is never
                    // contacted; name it with its excluded-key summary and
                    // continue the request on the next candidate.
                    let reason = excluded_key_summary(&candidate.provider, request_has_enabled_key);
                    record_provider_failure_if_absent(
                        &mut failures,
                        &failure_label(&candidate.provider),
                        &reason,
                    );
                    continue;
                };
                (candidate, retries.len(), None, selected, None)
            } else if let Some(index) =
                RetryCandidate::next_ready(&retries, &mut remaining_wait).await
            {
                let mut candidate = retries.remove(index);
                sync_key_runtime_marks(&mut candidate.provider);
                let Some(selected) = select_attempt_key(
                    &candidate.provider,
                    now_ts(),
                    key_probe_used,
                    &candidate.attempted_keys,
                ) else {
                    // No usable key remains for this retry candidate; name it
                    // with its excluded-key summary and drop it.
                    let reason = excluded_key_summary(&candidate.provider, request_has_enabled_key);
                    record_provider_failure_if_absent(
                        &mut failures,
                        &failure_label(&candidate.provider),
                        &reason,
                    );
                    continue;
                };
                (candidate, index, None, selected, None)
            } else if let Some(probe_candidate) = pending_probe.take() {
                // The single-flight guard is acquired once and then held across
                // every in-probe key rotation, so a re-entry must not re-acquire it.
                if _probe_guard.is_none() {
                    match try_acquire_probe_guard(&probe_candidate.target) {
                        Some(guard) => _probe_guard = Some(guard),
                        // Another request already probes this mapping key: skip
                        // this attempt and let the next fallback source (if any)
                        // continue the request.
                        None => continue,
                    }
                }
                let mut provider = probe_candidate.provider.clone();
                sync_key_runtime_marks(&mut provider);
                let probe_attempted: HashSet<String> = HashSet::new();
                let Some(selected) =
                    select_attempt_key(&provider, now_ts(), key_probe_used, &probe_attempted)
                else {
                    // No usable key remains after consuming the key-scoped failures:
                    // re-arm the probe cooldown and let the next fallback source (if
                    // any) continue the request.
                    if let Some(reason) = probe_key_failure_reason.take() {
                        health.record_probe_failure(
                            &probe_candidate.target,
                            now_ts(),
                            false,
                            &reason,
                        );
                        record_provider_failure(&mut failures, &failure_label(&provider), reason);
                    }
                    continue;
                };
                (
                    RetryCandidate {
                        provider,
                        model: probe_candidate.upstream_model.clone(),
                        attempts: 0,
                        ready_at: None,
                        attempted_keys: probe_attempted,
                    },
                    retries.len(),
                    Some(probe_candidate.target.clone()),
                    selected,
                    Some(probe_candidate.protocol),
                )
            } else if let Some(stage) = pending_fallback.pop_front() {
                // The regular stage is exhausted. A queued source already
                // attempted by this request is skipped so the same model's
                // remaining sources stay eligible; an attempted source receives
                // one pass with the same request-wide key-probe allowance and is
                // never rescheduled for a delayed retry.
                if let Some(key) = fallback_stage_key(&stage) {
                    if attempted_sources.contains(&key) {
                        continue;
                    }
                }
                fallback_active = true;
                match stage {
                    PoolFallbackStage::Ready(snapshot) => {
                        let mut provider = snapshot.clone();
                        sync_key_runtime_marks(&mut provider);
                        let Some((model, _)) = attempt_target(&provider, requested, protocol)
                        else {
                            record_provider_failure_if_absent(
                                &mut failures,
                                &failure_label(&provider),
                                "fallback source cannot serve this endpoint",
                            );
                            continue;
                        };
                        let attempted_keys: HashSet<String> = HashSet::new();
                        let Some(selected) = select_attempt_key(
                            &provider,
                            now_ts(),
                            key_probe_used,
                            &attempted_keys,
                        ) else {
                            let reason = excluded_key_summary(&provider, true);
                            record_provider_failure_if_absent(
                                &mut failures,
                                &failure_label(&provider),
                                &reason,
                            );
                            continue;
                        };
                        (
                            RetryCandidate {
                                provider,
                                model,
                                attempts: 0,
                                ready_at: None,
                                attempted_keys,
                            },
                            retries.len(),
                            None,
                            selected,
                            None,
                        )
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
                                    continue;
                                }
                            }
                        }
                        let mut provider = candidate.provider.clone();
                        sync_key_runtime_marks(&mut provider);
                        let attempted_keys: HashSet<String> = HashSet::new();
                        let Some(selected) = select_attempt_key(
                            &provider,
                            now_ts(),
                            key_probe_used,
                            &attempted_keys,
                        ) else {
                            record_provider_failure_if_absent(
                                &mut failures,
                                &failure_label(&provider),
                                "fallback probe has no usable key",
                            );
                            continue;
                        };
                        (
                            RetryCandidate {
                                provider,
                                model: candidate.upstream_model.clone(),
                                attempts: 0,
                                ready_at: None,
                                attempted_keys,
                            },
                            retries.len(),
                            Some(candidate.target.clone()),
                            selected,
                            Some(candidate.protocol),
                        )
                    }
                }
            } else {
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
        // A probe forwards under the probed row's own protocol (the row is
        // auto-disabled, so the normal served-row resolution skips it); every
        // other attempt keeps the resolved effective protocol.
        let upstream_protocol = attempt_protocol
            .or_else(|| {
                attempt_target(&candidate.provider, requested, protocol)
                    .map(|(_, upstream)| upstream)
            })
            .unwrap_or(protocol);
        let native_messages = upstream_protocol == UpstreamProtocol::AnthropicMessages;
        let mut native_sse = native_messages.then(AnthropicSseObserver::default);
        let mut converted_sse = (native_messages && is_conversion_path(path))
            .then(|| AnthropicSseConverter::new(path, requested.unwrap_or(&candidate.model)));
        // A prepared model-pool candidate on an OpenAI endpoint gets an OpenAI
        // SSE converter so the caller's own protocol and the public alias are
        // preserved even on a same-protocol stream.
        let mut pool_sse = (candidate.provider.pool_candidate.is_some()
            && !native_messages
            && is_conversion_path(path))
        .then(|| {
            OpenAiSseConverter::new(
                path,
                upstream_protocol,
                requested.unwrap_or(&candidate.model),
                include_usage,
            )
        });
        capture.provider_id = candidate.provider.id.clone();
        capture.provider_name = candidate.provider.name.clone();
        capture.upstream_model = candidate.model.clone();
        capture.protocol = upstream_protocol;
        let provider = &candidate.provider;
        let started = Instant::now();
        // Fresh per-attempt observation of the raw upstream protocol bytes. It
        // is fed the same chunks used for usage capture, before awaited writes,
        // and is finished before health settlement and persistence so the
        // whole-attempt window excludes downstream and bookkeeping time.
        let mut output_timing = OutputTimingObserver::new(upstream_protocol, started);
        // Caller-protocol tool identities observed incrementally from the bytes
        // actually written to the caller, so a completed pool stream can publish
        // or close the same-turn continuation preference without buffering the
        // answer. Only a prepared pool candidate needs this evidence.
        let mut pool_tools = provider
            .pool_candidate
            .is_some()
            .then(|| pool_context_hold::StreamingToolObserver::new(path));
        let (
            class,
            retryable,
            reason,
            retry_delay,
            transport,
            key_failure,
            bare_rotation,
            pool_scope,
        ) = 'attempt: {
            let streamed = open_streaming_response_with_protocol(
                provider,
                path,
                body,
                &candidate.model,
                client_headers,
                upstream_protocol,
                settings,
            )
            .await;
            let response = match streamed {
                Ok(response) => response,
                Err(error) => {
                    last_failure_status = Some(0);
                    let reason = format_network_error_reason(&error);
                    attempts.push(build_attempt_log(
                        provider,
                        upstream_protocol,
                        &candidate.model,
                        started,
                        0,
                        UsageResult::Failure,
                        sanitize_provider_error_text(&reason, provider),
                        None,
                        None,
                        None,
                    ));
                    break 'attempt (
                        FailureClass::Retryable,
                        true,
                        reason,
                        None,
                        true,
                        None,
                        false,
                        false,
                    );
                }
            };
            let status = response.status().as_u16();
            let retry_delay = retry_header_delay(response.headers());
            if status >= 400 {
                // An unreadable error body must not override the known HTTP
                // status (especially immediate disabling for 401/403).
                let bytes = response.bytes().await.unwrap_or_default();
                let parsed = serde_json::from_slice::<Value>(&bytes).is_ok();
                let error_message = sanitize_provider_error_text(
                    &extract_upstream_error_text(&bytes).unwrap_or_default(),
                    provider,
                );
                let class = classify_attempt_failure(
                    status,
                    false,
                    parsed,
                    error_message.as_deref(),
                    provider.pool_candidate.is_some(),
                );
                if class == FailureClass::ReturnToClient {
                    let reason = failure_reason(status, parsed, error_message.as_deref());
                    if let Some(target) = &probe_target {
                        // A probe that returns the upstream 4xx unchanged still
                        // re-arms its cooldown (REQ-001).
                        health.record_probe_failure(target, now_ts(), false, &reason);
                    }
                    if selected.probe {
                        persist_key_runtime_state(&provider.id, &selected.id, |key| {
                            rearm_key_probe(key, None, &reason, now_ts());
                        });
                    }
                    health.apply();
                    capture.status = status;
                    attempts.push(build_attempt_log(
                        provider,
                        protocol,
                        &candidate.model,
                        started,
                        status,
                        UsageResult::Failure,
                        error_message,
                        None,
                        None,
                        None,
                    ));
                    // Byte-for-byte only when the upstream body is already a
                    // standard error; otherwise keep the status and wrap it
                    // (REQ-004/AC-006/AC-007).
                    let standard = is_standard_error_body(&bytes);
                    let body = if native_messages || standard {
                        bytes.to_vec()
                    } else {
                        serde_json::to_vec(&upstream_error_payload(status, &bytes))
                            .unwrap_or_else(|_| b"{}".to_vec())
                    };
                    write_response(
                        writer,
                        HttpResponse {
                            status,
                            content_type: "application/json",
                            body,
                            capture: None,
                        },
                    )
                    .await?;
                    return Ok(capture);
                }
                last_failure_status = Some(status);
                let reason = failure_reason(status, parsed, error_message.as_deref());
                // Quota-exhausted 429s count toward health but never requeue
                // the same provider; compute before `error_message` is moved
                // into the attempt log.
                let retryable = is_retryable_with_message(class, status, error_message.as_deref());
                let key_failure = key_failure_kind(status, error_message.as_deref());
                let bare_rotation = status == 429 && key_failure.is_none();
                let error_type = extract_upstream_error_type(&bytes);
                let pool_scope = provider.pool_candidate.is_some()
                    && pool_candidate_scope_failure(
                        status,
                        error_message.as_deref(),
                        error_type.as_deref(),
                        false,
                    );
                attempts.push(build_attempt_log(
                    provider,
                    protocol,
                    &candidate.model,
                    started,
                    status,
                    UsageResult::Failure,
                    error_message,
                    None,
                    None,
                    None,
                ));
                break 'attempt (
                    class,
                    retryable,
                    reason,
                    retry_delay,
                    false,
                    key_failure,
                    bare_rotation,
                    pool_scope,
                );
            }

            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("")
                .to_string();
            let mut chunks = response.bytes_stream();
            match chunks.next().await {
                Some(Ok(first)) => {
                    // Timestamp receipt before any awaited downstream write or
                    // conversion so the observation reflects the real upstream
                    // arrival of this chunk.
                    output_timing.feed_at(&first, Instant::now());
                    // A pool conversion reads the stream incrementally, so a
                    // first chunk fragmented inside an event's `event:` field
                    // (e.g. `even`) must not be rejected; a declared SSE content
                    // type is sufficient and a truly malformed stream becomes a
                    // post-byte failure.
                    let pool_sse_stream = pool_sse.is_some()
                        && content_type
                            .to_ascii_lowercase()
                            .contains("text/event-stream");
                    if !is_sse_response_chunk(&content_type, &first) && !pool_sse_stream {
                        last_failure_status = Some(502);
                        let reason = format!(
                            "2xx response is not a valid SSE stream (content-type: {content_type})"
                        );
                        // The rejected 2xx body is the only readable failure
                        // information this attempt produces (REQ-003).
                        attempts.push(build_attempt_log(
                            provider,
                            protocol,
                            &candidate.model,
                            started,
                            502,
                            UsageResult::Failure,
                            sanitize_provider_error_text(
                                &extract_upstream_error_text(&first).unwrap_or_default(),
                                provider,
                            ),
                            None,
                            None,
                            None,
                        ));
                        break 'attempt (
                            FailureClass::Retryable,
                            true,
                            reason,
                            retry_delay,
                            false,
                            None,
                            false,
                            false,
                        );
                    }
                    if !native_messages || converted_sse.is_some() {
                        if let Err(error) = write_stream_headers(writer, status).await {
                            health.apply();
                            return Err(error);
                        }
                    }
                    // Capture usage from a read-only copy before the bytes are
                    // written; the forwarded payload is unchanged.
                    usage.feed(&first);
                    if let Some(observer) = native_sse.as_mut() {
                        observer.feed(&first);
                    }
                    // A conversion error after the first upstream byte is a
                    // post-byte failure: no success marker, no retry, and the
                    // failed attempt keeps its captured usage.
                    let mut stream_error: Option<String> = None;
                    let first_output = if let Some(converter) = converted_sse.as_mut() {
                        match converter.feed(&first) {
                            Ok(output) => output,
                            Err(error) => {
                                stream_error =
                                    Some(format!("stream failed after first byte: {error}"));
                                Vec::new()
                            }
                        }
                    } else if let Some(converter) = pool_sse.as_mut() {
                        match converter.feed(&first) {
                            Ok(output) => output,
                            Err(error) => {
                                stream_error =
                                    Some(format!("stream failed after first byte: {error}"));
                                Vec::new()
                            }
                        }
                    } else {
                        vec![first.to_vec()]
                    };
                    if stream_error.is_none() {
                        if let Some(observer) = pool_tools.as_mut() {
                            for output in &first_output {
                                observer.feed(output);
                            }
                        }
                        for output in &first_output {
                            if let Err(error) = writer.write_all(output).await {
                                health.apply();
                                return Err(error.to_string());
                            }
                        }
                    }
                    // Last byte forwarded to the caller, used to complete the
                    // SSE event boundary before a mid-stream error fragment
                    // (REQ-005/AC-008).
                    let mut last_forwarded = first_output
                        .last()
                        .and_then(|output| output.last().copied());
                    if stream_error.is_none() && writer.flush().await.is_err() {
                        health.apply();
                        capture.downstream_cancelled = true;
                        return Ok(capture);
                    }
                    loop {
                        // Bytes already sent: terminate the stream, never switch.
                        // The failed attempt keeps the usage accumulated so far
                        // (REQ-004/REQ-005/AC-009).
                        if let Some(reason) = stream_error.take() {
                            // The whole-attempt window ends at the failure,
                            // before health settlement, persistence or the
                            // downstream error fragment.
                            output_timing.finish_at(Instant::now());
                            if let Some(target) = &probe_target {
                                // A mid-stream probe failure re-arms the
                                // cooldown; it is never retried and no
                                // second probe is attempted (REQ-001).
                                health.record_probe_failure(target, now_ts(), true, &reason);
                            } else {
                                settle_failure(
                                    &mut health,
                                    provider,
                                    requested,
                                    &candidate.model,
                                    FailureClass::Retryable,
                                    &reason,
                                    true,
                                );
                            }
                            if selected.probe {
                                persist_key_runtime_state(&provider.id, &selected.id, |key| {
                                    rearm_key_probe(key, None, &reason, now_ts());
                                });
                            }
                            health.apply();
                            // The attempt is complete: its stream ended with
                            // an error and keeps the usage accumulated so far
                            // (REQ-003/REQ-005).
                            let failed_usage = if native_messages {
                                native_sse
                                    .as_ref()
                                    .and_then(AnthropicSseObserver::canonical_usage)
                            } else {
                                usage.canonical_usage()
                            };
                            // A failed stream keeps the first-output measurement
                            // and any TPS derivable from the cumulative output
                            // usage actually observed before it failed.
                            let (first_token_ms, avg_tps) = output_timing.timing(
                                failed_usage.and_then(|canonical| canonical.reported_output_tokens),
                            );
                            attempts.push(build_attempt_log(
                                provider,
                                protocol,
                                &candidate.model,
                                started,
                                502,
                                UsageResult::Failure,
                                sanitize_provider_error_text(&reason, provider),
                                failed_usage,
                                first_token_ms,
                                avg_tps,
                            ));
                            // Complete the SSE event boundary so the error
                            // fragment parses standalone even when the last
                            // forwarded byte is not a newline, then append one
                            // `data:` error event. No `[DONE]`, no retry, no
                            // candidate switch; downstream write failures stay
                            // best-effort (REQ-005/AC-008).
                            if last_forwarded != Some(b'\n') {
                                let _ = writer.write_all(b"\n").await;
                            }
                            let _ = writer.write_all(b"\n").await;
                            if is_messages_path(path) {
                                let fragment = stream_error_event(reason, "api_error");
                                let _ = writer.write_all(&fragment).await;
                            } else {
                                let envelope =
                                    error_envelope(reason, "server_error", "upstream_stream_error");
                                // A converted pool Responses client gets the
                                // Responses error event shape; the ordinary
                                // passthrough keeps its existing `data:` fragment.
                                let fragment = if pool_sse.is_some()
                                    && path.split('?').next() == Some("/v1/responses")
                                {
                                    format!("event: error\ndata: {envelope}\n\n")
                                } else {
                                    format!("data: {envelope}\n\n")
                                };
                                let _ = writer.write_all(fragment.as_bytes()).await;
                            }
                            let _ = writer.flush().await;
                            capture.status = 502;
                            capture.usage = if native_messages {
                                failed_usage.map(|canonical| canonical.tokens)
                            } else {
                                usage.usage()
                            };
                            capture.upstream_error = true;
                            return Ok(capture);
                        }
                        match chunks.next().await {
                            Some(Ok(chunk)) => {
                                output_timing.feed_at(&chunk, Instant::now());
                                usage.feed(&chunk);
                                if let Some(observer) = native_sse.as_mut() {
                                    observer.feed(&chunk);
                                }
                                let output = if let Some(converter) = converted_sse.as_mut() {
                                    match converter.feed(&chunk) {
                                        Ok(output) => output,
                                        Err(error) => {
                                            stream_error = Some(format!(
                                                "stream failed after first byte: {error}"
                                            ));
                                            Vec::new()
                                        }
                                    }
                                } else if let Some(converter) = pool_sse.as_mut() {
                                    match converter.feed(&chunk) {
                                        Ok(output) => output,
                                        Err(error) => {
                                            stream_error = Some(format!(
                                                "stream failed after first byte: {error}"
                                            ));
                                            Vec::new()
                                        }
                                    }
                                } else {
                                    vec![chunk.to_vec()]
                                };
                                if stream_error.is_some() {
                                    continue;
                                }
                                if let Some(observer) = pool_tools.as_mut() {
                                    for item in &output {
                                        observer.feed(item);
                                    }
                                }
                                for item in &output {
                                    if writer.write_all(item).await.is_err() {
                                        health.apply();
                                        capture.downstream_cancelled = true;
                                        return Ok(capture);
                                    }
                                }
                                last_forwarded = output
                                    .last()
                                    .and_then(|item| item.last().copied())
                                    .or(last_forwarded);
                            }
                            Some(Err(error)) => {
                                output_timing.finish_at(Instant::now());
                                stream_error =
                                    Some(format!("stream failed after first byte: {error}"));
                                continue;
                            }
                            None => {
                                // The upstream stream ended: close the
                                // whole-attempt window before health settlement,
                                // persistence and downstream completion
                                // bookkeeping.
                                output_timing.finish_at(Instant::now());
                                let native_usage = if native_messages {
                                    let mut observer =
                                        native_sse.take().expect("native Messages stream observer");
                                    observer.finish();
                                    if !observer.is_success() {
                                        let reason = if observer.has_in_band_error() {
                                            "upstream emitted an Anthropic error event".to_string()
                                        } else {
                                            "native stream ended before message_stop".to_string()
                                        };
                                        if let Some(target) = &probe_target {
                                            health.record_probe_failure(
                                                target,
                                                now_ts(),
                                                false,
                                                &reason,
                                            );
                                        } else {
                                            settle_failure(
                                                &mut health,
                                                provider,
                                                requested,
                                                &candidate.model,
                                                FailureClass::Retryable,
                                                &reason,
                                                false,
                                            );
                                        }
                                        if selected.probe {
                                            persist_key_runtime_state(
                                                &provider.id,
                                                &selected.id,
                                                |key| rearm_key_probe(key, None, &reason, now_ts()),
                                            );
                                        }
                                        health.apply();
                                        let canonical = observer.canonical_usage();
                                        let (first_token_ms, avg_tps) =
                                            output_timing
                                                .timing(canonical.and_then(|usage| {
                                                    usage.reported_output_tokens
                                                }));
                                        attempts.push(build_attempt_log(
                                            provider,
                                            protocol,
                                            &candidate.model,
                                            started,
                                            502,
                                            UsageResult::Failure,
                                            sanitize_provider_error_text(&reason, provider),
                                            canonical,
                                            first_token_ms,
                                            avg_tps,
                                        ));
                                        if !observer.has_in_band_error() {
                                            if is_messages_path(path) {
                                                let fragment =
                                                    stream_error_event(reason, "api_error");
                                                let _ = writer.write_all(&fragment).await;
                                            } else {
                                                let envelope = error_envelope(
                                                    reason,
                                                    "server_error",
                                                    "upstream_stream_error",
                                                );
                                                let _ = writer
                                                    .write_all(
                                                        format!("data: {envelope}\n\n").as_bytes(),
                                                    )
                                                    .await;
                                            }
                                            let _ = writer.flush().await;
                                        }
                                        capture.status = 502;
                                        capture.usage = canonical.map(|usage| usage.tokens);
                                        capture.upstream_error = true;
                                        return Ok(capture);
                                    }
                                    observer.canonical_usage()
                                } else {
                                    None
                                };
                                // Flush a final native event that arrived
                                // without a trailing blank line (for example
                                // `message_stop` immediately before EOF) so the
                                // converted caller still receives its single
                                // completion marker (REQ-004).
                                if let Some(mut converter) = converted_sse.take() {
                                    if let Ok(outputs) = converter.finish() {
                                        if let Some(observer) = pool_tools.as_mut() {
                                            for output in &outputs {
                                                observer.feed(output);
                                            }
                                        }
                                        for output in &outputs {
                                            let _ = writer.write_all(output).await;
                                        }
                                        let _ = writer.flush().await;
                                    }
                                }
                                // A converted OpenAI pool stream must have seen
                                // its completion marker; a truncated or malformed
                                // stream is a post-byte failure, never a success.
                                if let Some(mut converter) = pool_sse.take() {
                                    let finished = converter.finish();
                                    if let Ok(outputs) = &finished {
                                        if let Some(observer) = pool_tools.as_mut() {
                                            for output in outputs {
                                                observer.feed(output);
                                            }
                                        }
                                        for output in outputs {
                                            let _ = writer.write_all(output).await;
                                        }
                                        let _ = writer.flush().await;
                                    }
                                    if finished.is_err() || !converter.is_success() {
                                        let reason = if converter.has_in_band_error() {
                                            "upstream emitted an error event".to_string()
                                        } else if let Err(error) = &finished {
                                            format!("stream failed after first byte: {error}")
                                        } else {
                                            "converted stream ended before completion".to_string()
                                        };
                                        if let Some(target) = &probe_target {
                                            health.record_probe_failure(
                                                target,
                                                now_ts(),
                                                false,
                                                &reason,
                                            );
                                        } else {
                                            settle_failure(
                                                &mut health,
                                                provider,
                                                requested,
                                                &candidate.model,
                                                FailureClass::Retryable,
                                                &reason,
                                                false,
                                            );
                                        }
                                        if selected.probe {
                                            persist_key_runtime_state(
                                                &provider.id,
                                                &selected.id,
                                                |key| {
                                                    rearm_key_probe(key, None, &reason, now_ts());
                                                },
                                            );
                                        }
                                        health.apply();
                                        let failed_usage = usage.canonical_usage();
                                        let (first_token_ms, avg_tps) =
                                            output_timing.timing(failed_usage.and_then(
                                                |canonical| canonical.reported_output_tokens,
                                            ));
                                        attempts.push(build_attempt_log(
                                            provider,
                                            protocol,
                                            &candidate.model,
                                            started,
                                            502,
                                            UsageResult::Failure,
                                            sanitize_provider_error_text(&reason, provider),
                                            failed_usage,
                                            first_token_ms,
                                            avg_tps,
                                        ));
                                        if last_forwarded != Some(b'\n') {
                                            let _ = writer.write_all(b"\n").await;
                                        }
                                        let _ = writer.write_all(b"\n").await;
                                        let envelope = error_envelope(
                                            reason,
                                            "server_error",
                                            "upstream_stream_error",
                                        );
                                        let fragment =
                                            if path.split('?').next() == Some("/v1/responses") {
                                                format!("event: error\ndata: {envelope}\n\n")
                                            } else {
                                                format!("data: {envelope}\n\n")
                                            };
                                        let _ = writer.write_all(fragment.as_bytes()).await;
                                        let _ = writer.flush().await;
                                        capture.status = 502;
                                        capture.usage = usage.usage();
                                        capture.upstream_error = true;
                                        return Ok(capture);
                                    }
                                }
                                if let Some(target) = &probe_target {
                                    // A served probe clears its row's runtime
                                    // state (REQ-001/AC-001).
                                    health.record_probe_success(target);
                                } else {
                                    settle_success(
                                        &mut health,
                                        provider,
                                        requested,
                                        &candidate.model,
                                        upstream_protocol,
                                    );
                                }
                                if selected.probe {
                                    persist_key_runtime_state(
                                        &provider.id,
                                        &selected.id,
                                        clear_key_runtime_state,
                                    );
                                }
                                health.apply();
                                capture.status = status;
                                capture.fallback_stage = fallback_active;
                                capture.usage = native_usage
                                    .map(|canonical| canonical.tokens)
                                    .or_else(|| usage.usage());
                                // A normally ended stream is the served success
                                // of this attempt (REQ-001/REQ-003). TPS uses the
                                // same authoritative cumulative usage selection
                                // as logging and its independently validated
                                // reported output count.
                                let success_usage =
                                    native_usage.or_else(|| usage.canonical_usage());
                                let (first_token_ms, avg_tps) =
                                    output_timing
                                        .timing(success_usage.and_then(|canonical| {
                                            canonical.reported_output_tokens
                                        }));
                                attempts.push(build_attempt_log(
                                    provider,
                                    protocol,
                                    &candidate.model,
                                    started,
                                    status,
                                    UsageResult::Success,
                                    None,
                                    success_usage,
                                    first_token_ms,
                                    avg_tps,
                                ));
                                // A completed, delivered stream is the served
                                // success of this pool candidate: publish its
                                // exact identity and the caller-protocol tool
                                // identities observed on the client stream. A
                                // failed, incomplete or cancelled stream never
                                // reaches here and so never publishes.
                                if let Some(observer) = pool_tools.take() {
                                    capture.pool_candidate = provider.pool_candidate.clone();
                                    capture.endpoint = provider.base_url.clone();
                                    capture.served_tool_ids = observer.finish();
                                }
                                return Ok(capture);
                            }
                        }
                    }
                }
                Some(Err(error)) => {
                    last_failure_status = Some(0);
                    let reason = format!("stream failed before first byte: {error}");
                    attempts.push(build_attempt_log(
                        provider,
                        protocol,
                        &candidate.model,
                        started,
                        0,
                        UsageResult::Failure,
                        sanitize_provider_error_text(&reason, provider),
                        None,
                        None,
                        None,
                    ));
                    break 'attempt (
                        FailureClass::Retryable,
                        true,
                        reason,
                        retry_delay,
                        true,
                        None,
                        false,
                        false,
                    );
                }
                None => {
                    last_failure_status = Some(502);
                    let reason = "upstream returned an empty stream".to_string();
                    // No readable body ever arrived, so this attempt stores no
                    // error message (REQ-003).
                    attempts.push(build_attempt_log(
                        provider,
                        protocol,
                        &candidate.model,
                        started,
                        502,
                        UsageResult::Failure,
                        None,
                        None,
                        None,
                        None,
                    ));
                    break 'attempt (
                        FailureClass::Retryable,
                        true,
                        reason,
                        retry_delay,
                        false,
                        None,
                        false,
                        false,
                    );
                }
            }
        };
        // Model-pool scope (REQ-006): a model-scoped or unknown-scope
        // quota-like failure cools only this candidate and drops it so the loop
        // continues on the next selected candidate without marking the shared
        // key, exhausting its keys or counting mapping health. `pool_scope` is
        // never set for an ordinary attempt, whose rotation below is unchanged.
        if pool_scope {
            if let Some(identity) = provider.pool_candidate.as_ref() {
                cool_pool_candidate(identity, retry_delay);
            }
            record_provider_failure(&mut failures, &failure_label(&provider), reason);
            continue;
        }
        if let Some(target) = &probe_target {
            if !selected.probe {
                if let Some(kind) = key_failure {
                    // A normally selected usable key failed auth/quota before the
                    // first byte inside the probe: mark it and continue the same
                    // probe on the provider's next usable key, without registering
                    // mapping health or consuming the retry budget.
                    let at = now_ts();
                    persist_key_runtime_state(&provider.id, &selected.id, |key| {
                        mark_key_failure(key, kind, &reason, at);
                    });
                    let mut next_provider = candidate.provider.clone();
                    if let Some(key) = next_provider
                        .keys
                        .iter_mut()
                        .find(|key| key.id == selected.id)
                    {
                        mark_key_failure(key, kind, &reason, at);
                    }
                    record_provider_failure(
                        &mut failures,
                        &failure_label(&provider),
                        reason.clone(),
                    );
                    probe_key_failure_reason = Some(reason);
                    pending_probe = Some(ProbeCandidate {
                        provider: next_provider,
                        target: target.clone(),
                        upstream_model: candidate.model.clone(),
                        protocol: upstream_protocol,
                    });
                    continue;
                }
            }
            // A probe that failed before its first byte re-arms its cooldown,
            // names the provider in the all-unavailable message and is never
            // queued for retry or backoff (REQ-001/AC-002).
            health.record_probe_failure(target, now_ts(), transport, &reason);
            if selected.probe {
                persist_key_runtime_state(&provider.id, &selected.id, |key| {
                    rearm_key_probe(key, key_failure, &reason, now_ts());
                });
            }
            record_provider_failure(&mut failures, &failure_label(&provider), reason);
        } else if selected.probe {
            // A failed quota probe re-arms its cooldown and is never rotated or
            // requeued, exactly like the mapping probe.
            persist_key_runtime_state(&provider.id, &selected.id, |key| {
                rearm_key_probe(key, key_failure, &reason, now_ts());
            });
            record_provider_failure(&mut failures, &failure_label(&provider), reason);
        } else if let Some(kind) = key_failure {
            // Key-scoped failure before the first byte: mark the key and retry
            // the same provider immediately on its next usable key, consuming
            // no retry budget and registering no mapping health.
            let at = now_ts();
            persist_key_runtime_state(&provider.id, &selected.id, |key| {
                mark_key_failure(key, kind, &reason, at);
            });
            record_provider_failure(&mut failures, &failure_label(&provider), reason.clone());
            let mut next_provider = candidate.provider.clone();
            if let Some(key) = next_provider
                .keys
                .iter_mut()
                .find(|key| key.id == selected.id)
            {
                mark_key_failure(key, kind, &reason, at);
            }
            candidate.provider = next_provider;
            candidate.attempted_keys.insert(selected.id.clone());
            // Continue the same pass on the provider's next selectable key — an
            // unattempted usable key or, only when none remains and no probe has
            // run yet, an eligible 60-second quota probe. When neither exists the
            // pass simply ends. (The bare-rotation branch below stays
            // usable-key-only so probes remain last-resort.)
            if select_attempt_key(
                &candidate.provider,
                now_ts(),
                key_probe_used,
                &candidate.attempted_keys,
            )
            .is_some()
            {
                ordered_queue.push_front(candidate);
            }
        } else if bare_rotation {
            // A bare rate-limit 429 rotates within the pass without persisting a
            // mark, registering mapping health or consuming the retry budget.
            candidate.attempted_keys.insert(selected.id.clone());
            record_provider_failure(&mut failures, &failure_label(&provider), reason);
            if select_usable_key(&candidate.provider, now_ts(), &candidate.attempted_keys).is_some()
            {
                ordered_queue.push_front(candidate);
            } else {
                // The pass exhausted every usable key: hand the failure to the
                // bounded retry scheduler with the full budget.
                candidate.attempts += 1;
                if !fallback_active && retryable && candidate.attempts <= settings.max_retries_per_provider {
                    candidate.ready_at = Instant::now().checked_add(
                        retry_delay.unwrap_or_else(|| default_retry_delay_with_base(candidate.attempts, settings.retry_base_delay_ms)),
                    );
                    candidate.attempted_keys.clear();
                    retries.insert(retry_index, candidate);
                }
            }
        } else {
            settle_failure(
                &mut health,
                provider,
                requested,
                &candidate.model,
                class,
                &reason,
                transport,
            );
            record_provider_failure(&mut failures, &failure_label(&provider), reason);
            candidate.attempts += 1;
            if !fallback_active && retryable && candidate.attempts <= settings.max_retries_per_provider {
                candidate.ready_at = Instant::now().checked_add(
                    retry_delay.unwrap_or_else(|| default_retry_delay_with_base(candidate.attempts, settings.retry_base_delay_ms)),
                );
                candidate.attempted_keys.clear();
                retries.insert(retry_index, candidate);
            }
        }
    }

    health.apply();
    // Nothing was written downstream yet, so the failure is a plain HTTP 502
    // JSON response rather than an SSE error event over HTTP 200 (REQ-003).
    let message = all_unavailable_message(&failures);
    let response = json_response(502, all_unavailable_payload_for_path(path, message.clone()));
    write_response(writer, response).await?;
    // The transport is 502, but the log records the real upstream failure status
    // (0 when no upstream HTTP status was determinable, for example a network
    // error) rather than the transport's status.
    capture.status = last_failure_status.unwrap_or(502);
    capture.usage = None;
    capture.all_unavailable = true;
    // The terminal row records the same diagnostic the caller received.
    capture.message = Some(message);
    Ok(capture)
}


/// Forward one streaming request using the attempt loop.
pub(super) async fn attempt_streaming_request<W: AsyncWrite + Unpin>(
    writer: &mut W,
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
) -> Result<ForwardCapture, String> {
    attempt_streaming_inner(
        writer,
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

