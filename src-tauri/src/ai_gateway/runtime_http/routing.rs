use super::*;
use super::errors::{
    all_unavailable_payload_for_path, error_envelope, local_error_payload, no_candidate_message,
};
use super::http_io::{write_response, NativeHttpWriter};
use super::non_streaming::attempt_non_streaming_request;
use super::streaming::attempt_streaming_request;
use super::usage::{
    record_request_usage_logs, record_usage_log, synthetic_terminal_row,
};

fn clean_path(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

/// Normalize the incoming path to its canonical `/v1` form.
///
/// OpenAI-compatible clients disagree on whether the configured base URL
/// already ends in `/v1`, so `/chat/completions` and `/v1/chat/completions`
/// must both resolve; upstream always receives the versioned path.
fn canonical_api_path(path: &str) -> Option<&'static str> {
    match path {
        "/v1/models" | "/models" => Some("/v1/models"),
        "/v1/chat/completions" | "/chat/completions" => Some("/v1/chat/completions"),
        "/v1/responses" | "/responses" => Some("/v1/responses"),
        "/v1/messages" | "/messages" => Some("/v1/messages"),
        _ => None,
    }
}

/// The protocol an inbound canonical path speaks.
pub(super) fn protocol_for_path(path: &str) -> UpstreamProtocol {
    match path.split('?').next().unwrap_or(path) {
        "/v1/responses" => UpstreamProtocol::Responses,
        "/v1/messages" => UpstreamProtocol::AnthropicMessages,
        _ => UpstreamProtocol::ChatCompletions,
    }
}

/// Accept credentials from either `Authorization: Bearer <key>` or `x-api-key`.
/// Any enabled local key matches; no enabled key means every request is rejected.
pub(in crate::ai_gateway) fn is_authorized(request: &HttpRequest, config: &GatewayConfig) -> bool {
    let enabled: Vec<&GatewayKey> = config
        .keys
        .iter()
        .filter(|key| key.enabled && !key.value.trim().is_empty())
        .collect();
    if enabled.is_empty() {
        return false;
    }
    let bearer = request
        .headers
        .get("authorization")
        .and_then(|value| value.trim().strip_prefix("Bearer "))
        .map(str::trim);
    let x_api_key = request.headers.get("x-api-key").map(|value| value.trim());
    enabled.iter().any(|key| {
        let value = key.value.trim();
        bearer == Some(value) || x_api_key == Some(value)
    })
}

/// Union of local model names across enabled providers' user-enabled,
/// not-auto-disabled mapping rows, plus the public model id of every enabled
/// configured pool.
///
/// An enabled pool advertises its alias exactly once, independent of transient
/// candidate readiness/cooldown, key availability or a disabled source; a
/// disabled pool and a reserved-but-unowned retired alias are never advertised.
pub(in crate::ai_gateway) fn local_model_names(config: &GatewayConfig) -> Vec<String> {
    let mut names: Vec<String> = config
        .providers
        .iter()
        .filter(|provider| provider.enabled)
        .flat_map(|provider| {
            provider
                .mappings
                .iter()
                .filter(|mapping| mapping.enabled && !mapping.auto_disabled)
                .map(|mapping| mapping.local_model.trim().to_string())
        })
        .chain(
            config
                .model_pools
                .iter()
                .filter(|pool| pool.enabled)
                .map(|pool| pool.model.trim().to_string()),
        )
        .filter(|name| !name.is_empty())
        .collect();
    names.sort();
    names.dedup();
    names
}

pub(in crate::ai_gateway) fn models_payload(config: &GatewayConfig) -> Value {
    let data: Vec<Value> = local_model_names(config)
        .into_iter()
        .map(|id| {
            match config
                .model_pools
                .iter()
                .find(|pool| pool.enabled && pool.model.trim() == id.as_str())
            {
                Some(pool) => pool_discovery_entry(config, pool),
                None => json!({ "id": id, "object": "model" }),
            }
        })
        .collect();
    json!({ "object": "list", "data": data })
}

/// The `/v1/models` row for one enabled configured pool.
///
/// It advertises the pool's own public id and display name, the two OpenAI
/// ingress protocols it supports and its real selected source identities. It
/// additionally publishes the pool's *static* reasoning capability — the
/// supported canonical preferences, the canonical default and the five canonical
/// coverage rows — derived only from the stored configuration, so a temporarily
/// all-unready or cooling pool keeps the same advertised support. It never
/// carries a source key, endpoint, price, native mapped value or a
/// readiness/cooldown flag, so no capability is guessed from a source model.
fn pool_discovery_entry(config: &GatewayConfig, pool: &ModelPool) -> Value {
    let model = pool.model.trim();
    let name = pool.name.trim();
    let sources: Vec<Value> = pool
        .candidates
        .iter()
        .map(|candidate| {
            let provider_name = config
                .providers
                .iter()
                .find(|provider| provider.id == candidate.provider_id.trim())
                .map(|provider| provider.name.clone());
            json!({
                "provider_id": candidate.provider_id.trim(),
                "provider_name": provider_name,
                "local_model": candidate.local_model.trim(),
                "upstream_model": candidate.upstream_model.trim(),
                "protocol": candidate.protocol,
            })
        })
        .collect();
    let catalog = super::reasoning::pool_reasoning_catalog(&config.providers, pool);
    let reasoning_coverage: Vec<Value> = catalog
        .reasoning_coverage
        .iter()
        .map(|row| {
            json!({
                "effort": row.effort,
                "mapped_candidates": row.mapped_candidates,
                "total_candidates": row.total_candidates,
            })
        })
        .collect();
    json!({
        "id": model,
        "object": "model",
        "name": if name.is_empty() { model } else { name },
        "protocols": ["chat_completions", "responses"],
        "source_count": pool.candidates.len(),
        "sources": sources,
        "reasoning_efforts": catalog.reasoning_efforts,
        "default_reasoning_effort": catalog.default_reasoning_effort,
        "reasoning_coverage": reasoning_coverage,
    })
}

pub(in crate::ai_gateway) async fn handle_connection(mut stream: TcpStream) -> Result<(), String> {
    let request = match read_http_request(&mut stream).await {
        Ok(request) => request,
        Err(error) => {
            let response = json_response(
                400,
                error_envelope(error, "invalid_request_error", "invalid_request"),
            );
            let _ = stream.write_all(&http_response_bytes(response)).await;
            return Ok(());
        }
    };

    // Measured across the whole handled request so the recorded duration is
    // always within the gateway's processing time.
    let started = Instant::now();

    // Evaluate the post-resume grace exactly once, when the request starts:
    // every transport failure settled by this request is suppressed while the
    // window is open.
    let suppress_transport_failures =
        crate::app_runtime::system_resume_grace_active(SystemTime::now());

    let config = match read_config_shared() {
        Ok(config) => config,
        Err(error) => {
            // The route is known from the request line even when the
            // configuration cannot be read, so a native Messages early failure
            // still answers with the Anthropic error envelope.
            let response = json_response(
                500,
                local_error_payload(
                    clean_path(&request.path),
                    error,
                    "server_error",
                    "config_error",
                ),
            );
            let _ = stream.write_all(&http_response_bytes(response)).await;
            return Ok(());
        }
    };

    if request.method == "OPTIONS" && config.settings.enable_cors {
        let headers = "HTTP/1.1 204 No Content\r\n\
access-control-allow-origin: *\r\n\
access-control-allow-methods: GET, POST, OPTIONS\r\n\
access-control-allow-headers: authorization, x-api-key, content-type, accept, x-session-affinity, x-opencode-session, session-id, session_id, conversation_id, thread-id, x-session-id\r\n\
access-control-max-age: 86400\r\n\
content-length: 0\r\n\
connection: close\r\n\r\n";
        stream.write_all(headers.as_bytes()).await.map_err(|e| e.to_string())?;
        return Ok(());
    }

    let raw_path = clean_path(&request.path);
    let Some(canonical_path) = canonical_api_path(raw_path) else {
        let response = json_response(
            404,
            error_envelope(
                format!("unknown path: {raw_path}"),
                "invalid_request_error",
                "not_found",
            ),
        );
        stream
            .write_all(&http_response_bytes(response))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    };
    let upstream_path = if canonical_path == "/v1/messages" {
        request
            .path
            .split_once('?')
            .map(|(_, query)| format!("{canonical_path}?{query}"))
            .unwrap_or_else(|| canonical_path.to_string())
    } else {
        canonical_path.to_string()
    };
    let path = upstream_path.as_str();
    let is_models = path == "/v1/models";

    if !is_authorized(&request, &config) {
        let response = json_response(
            401,
            local_error_payload(
                path,
                "invalid or missing local API key",
                "invalid_request_error",
                "invalid_api_key",
            ),
        );
        stream
            .write_all(&http_response_bytes(response))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    if is_models {
        if request.method != "GET" {
            let response = json_response(
                404,
                error_envelope("not found", "invalid_request_error", "not_found"),
            );
            stream
                .write_all(&http_response_bytes(response))
                .await
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        let response = json_response(200, models_payload(&config));
        stream
            .write_all(&http_response_bytes(response))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    if request.method != "POST" {
        let response = json_response(
            404,
            local_error_payload(path, "not found", "invalid_request_error", "not_found"),
        );
        stream
            .write_all(&http_response_bytes(response))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    let body_value: Value = match serde_json::from_slice(&request.body) {
        Ok(value) => value,
        Err(error) => {
            let response = json_response(
                400,
                local_error_payload(
                    path,
                    error.to_string(),
                    "invalid_request_error",
                    "invalid_request",
                ),
            );
            stream
                .write_all(&http_response_bytes(response))
                .await
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
    };
    if is_messages_path(path)
        && body_value
            .get("model")
            .and_then(Value::as_str)
            .is_none_or(|model| model.trim().is_empty())
    {
        let response = json_response(
            400,
            local_error_payload(
                path,
                "field 'model' is required",
                "invalid_request_error",
                "invalid_request",
            ),
        );
        stream
            .write_all(&http_response_bytes(response))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let requested = body_value
        .get("model")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let reasoning_effort = extract_reasoning_effort(&body_value);
    let wants_stream = body_value
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let protocol = protocol_for_path(path);
    // Model-pool boundary: resolve an exact current or retired pool
    // alias before any ordinary candidate or provider default can serve it. A
    // disabled pool, a retired alias or a native Messages request for a pool
    // alias fails closed with no ordinary attempt.
    let pool_alias = requested
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    // The effective output cap of this request, used only to filter pool budget
    // mappings; computed before any selection.
    let pool_cap = pool_output_cap(path, &body_value);
    let mut pool_requested_pref: Option<PoolReasoningEffort> = None;
    let mut pool_resolved: Option<String> = None;
    // Recognize a current pool alias first, then parse its
    // reasoning intent and validate the cleaned portable contract before any
    // selection, SWRR mutation, key/probe or health change. A native Messages
    // alias stays on its fenced 502 path below.
    if let Some(alias) = pool_alias {
        if !is_messages_path(path) {
            if let Some(pool) = config
                .model_pools
                .iter()
                .find(|pool| pool.model.trim() == alias)
            {
                match parse_pool_request_effort(path, &body_value) {
                    Ok(preference) => pool_requested_pref = preference,
                    Err(message) => {
                        let response = json_response(
                            400,
                            local_error_payload(
                                path,
                                message,
                                "invalid_request_error",
                                "invalid_request",
                            ),
                        );
                        stream
                            .write_all(&http_response_bytes(response))
                            .await
                            .map_err(|e| e.to_string())?;
                        return Ok(());
                    }
                }
                pool_resolved = Some(resolved_effort_token(pool, pool_requested_pref));
                // A pool that stores at least one native Messages candidate must
                // also satisfy the existing adapter's contract, and a `strict`
                // function control it cannot carry is rejected before selection.
                let messages_candidate = pool
                    .candidates
                    .iter()
                    .any(|candidate| candidate.protocol == UpstreamProtocol::AnthropicMessages)
                    || pool.fallback_models.iter().any(|model| {
                        expand_fallback_model_in(&config.providers, model)
                            .iter()
                            .any(|candidate| {
                                candidate.protocol == UpstreamProtocol::AnthropicMessages
                            })
                    });
                // Validate the clean portable body with only this ingress's
                // recognized controls removed, so the existing adapters keep
                // their ordinary contract and every unrecognized reasoning
                // field is still rejected by name.
                let cleaned = strip_pool_entry_controls(path, &body_value);
                if let Err(error) =
                    openai_conversion::validate_pool_request(path, &cleaned, messages_candidate)
                {
                    let response = json_response(
                        400,
                        error_envelope(error, "invalid_request_error", "invalid_request"),
                    );
                    stream
                        .write_all(&http_response_bytes(response))
                        .await
                        .map_err(|e| e.to_string())?;
                    return Ok(());
                }
            }
        }
    }
    let pool_reasoning_request = PoolReasoningRequest {
        requested: pool_requested_pref,
        cap: pool_cap,
    };
    let pool_plan = pool_alias.and_then(|alias| {
        plan_pool_request(&config, alias, protocol, &pool_reasoning_request, now_ts())
    });
    // The ordered runtime classification: an unsupported
    // preference, a declared-but-unavailable preference and an output-cap
    // incompatibility are each answered before any attempt. A 400 is a request
    // validation error like every other local 400; the 502 unavailability shares
    // the fenced boundary's synthetic terminal row.
    if let Some(PoolPlan::Rejected {
        status,
        code,
        message,
    }) = &pool_plan
    {
        let error_type = if *status == 502 {
            "server_error"
        } else {
            "invalid_request_error"
        };
        let response = json_response(
            *status,
            local_error_payload(path, message.clone(), error_type, code),
        );
        stream
            .write_all(&http_response_bytes(response))
            .await
            .map_err(|e| e.to_string())?;
        if *status == 502 {
            record_usage_log(
                &config,
                &synthetic_terminal_row(
                    &config,
                    requested.as_deref().unwrap_or_default(),
                    UsageResult::Failure,
                    *status,
                    started.elapsed().as_millis().max(1) as u64,
                    Some(message),
                    reasoning_effort,
                    pool_resolved.clone(),
                ),
            )
            .await;
        }
        return Ok(());
    }
    if let Some(PoolPlan::Fenced { message }) = &pool_plan {
        let status = 502;
        let response = json_response(
            status,
            all_unavailable_payload_for_path(path, message.clone()),
        );
        stream
            .write_all(&http_response_bytes(response))
            .await
            .map_err(|e| e.to_string())?;
        // The alias is a real stored model boundary: record the same
        // nonempty source/model reason the caller received, with no upstream
        // attempt and no ordinary default.
        record_usage_log(
            &config,
            &synthetic_terminal_row(
                &config,
                requested.as_deref().unwrap_or_default(),
                UsageResult::Failure,
                status,
                started.elapsed().as_millis().max(1) as u64,
                Some(message),
                reasoning_effort,
                pool_resolved.clone(),
            ),
        )
        .await;
        return Ok(());
    }
    let mut pool_ordered = match &pool_plan {
        Some(PoolPlan::Ready { ordered, .. }) => Some(ordered.clone()),
        _ => None,
    };
    let pool_probe = match &pool_plan {
        Some(PoolPlan::Ready { probe, .. }) => probe.clone(),
        _ => None,
    };
    let pool_fallback = match &pool_plan {
        Some(PoolPlan::Ready { fallback, .. }) => fallback.clone(),
        _ => Vec::new(),
    };
    let is_pool = pool_ordered.is_some();

    // Same-turn pool continuation preference: resolve this
    // request's hold scope, admit it as the newest turn of that scope and move a
    // currently healthy matching candidate to the front of the planned pool
    // order. A request without a recognized scope, history or matching hold keeps
    // the planned order unchanged; the probe position, candidate set and alias
    // fencing are never touched.
    let hold_scope = if is_pool {
        pool_alias.and_then(|alias| {
            pool_context_hold::resolve_scope(&request.headers, &config, alias, protocol)
        })
    } else {
        None
    };
    let hold_admission = hold_scope.as_ref().map(pool_context_hold::admit);
    let hold_history = if is_pool {
        pool_context_hold::CallerHistory::parse(path, &body_value)
    } else {
        None
    };
    if let (Some(scope), Some(history), Some(ordered)) = (
        hold_scope.as_ref(),
        hold_history.as_ref(),
        pool_ordered.as_mut(),
    ) {
        pool_context_hold::prefer(&config, scope, history, ordered);
    }

    // The ordinary candidate set is only computed for non-pool requests; a pool
    // request routes exclusively through its resolved ordered candidates.
    let candidates: Vec<GatewayUpstreamProvider> = if is_pool {
        Vec::new()
    } else {
        candidate_providers(&config.providers, requested.as_deref(), protocol)
            .into_iter()
            .cloned()
            .collect()
    };
    // The half-open probe is computed against the healthy candidate provider
    // ids so an already-serving provider is never also probed. A pool
    // request probes only its own resolved candidate rows.
    let healthy_provider_ids: Vec<String> = candidates
        .iter()
        .map(|provider| provider.id.clone())
        .collect();
    let probe = if is_pool {
        pool_probe
    } else {
        find_probe_candidate_with_settings(
            &config.providers,
            requested.as_deref(),
            protocol,
            &healthy_provider_ids,
            now_ts(),
            config.settings.probe_cooldown_secs,
            config.settings.failure_threshold,
        )
    };
    // Conversion validation runs before any Anthropic attempt, including a
    // half-open probe: a probe-only Anthropic candidate must not bypass the
    // field-specific 400 and settle no probe/key health. A
    // model pool's candidate baseline is same-protocol only, so it is excluded.
    if !is_pool
        && !is_messages_path(path)
        && (candidates.iter().any(|provider| {
            resolve_upstream_protocol(provider, requested.as_deref(), protocol)
                == Some(UpstreamProtocol::AnthropicMessages)
        }) || probe
            .as_ref()
            .is_some_and(|candidate| candidate.protocol == UpstreamProtocol::AnthropicMessages))
    {
        if let Err(error) = validate_conversion_request(path, &body_value) {
            let response = json_response(
                400,
                error_envelope(error, "invalid_request_error", "invalid_request"),
            );
            stream
                .write_all(&http_response_bytes(response))
                .await
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    let probe_only = !is_pool && candidates.is_empty();
    if probe_only && probe.is_none() {
        let message = no_candidate_message(&config, requested.as_deref(), protocol);
        // Streaming and non-streaming alike answer HTTP 502 + JSON before any
        // byte is written; the log always records the gateway failure status.
        let status = 502;
        let response = json_response(502, all_unavailable_payload_for_path(path, message.clone()));
        stream
            .write_all(&http_response_bytes(response))
            .await
            .map_err(|e| e.to_string())?;
        // A request that entered the normalized flow but had no serving
        // upstream is a failure, never a silent no-log; it
        // keeps the existing single synthetic terminal row and writes it
        // through the single-row store call. The row records the same
        // no-candidate explanation the caller received.
        record_usage_log(
            &config,
            &synthetic_terminal_row(
                &config,
                requested.as_deref().unwrap_or_default(),
                UsageResult::Failure,
                status,
                started.elapsed().as_millis().max(1) as u64,
                Some(&message),
                reasoning_effort,
                None,
            ),
        )
        .await;
        return Ok(());
    }

    // A probe-only request neither reads nor writes a session-affinity binding
    // and uses no session id. A model-pool request never uses session
    // affinity: identical session headers must not pin which candidate serves,
    // and its candidates are already in SWRR order.
    let session_id = if probe_only || is_pool {
        None
    } else {
        resolve_session_id_with_custom(&request.headers, &config.settings.custom_session_headers)
    };
    let (ordered, bound_was_eligible) = if probe_only {
        (Vec::new(), false)
    } else if let Some(ordered) = pool_ordered {
        (ordered, false)
    } else {
        // The lookup, the shuffle and a first-request binding write share one
        // lock guard, which is released before any forwarding begins. A poisoned
        // binding table must not take the gateway down, so recover the guard
        // instead.
        let SessionOrder {
            ordered,
            bound_provider_id,
        } = session_affinity()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .resolve_order(session_id.as_deref(), requested.as_deref(), || {
                weighted_candidates(&candidates, requested.as_deref(), protocol)
            });
        // The reorder is a no-op when the bound provider is not one of this
        // request's eligible candidates, which forces the binding to be replaced.
        let bound_was_eligible = bound_provider_id
            .as_deref()
            .is_some_and(|id| candidates.iter().any(|provider| provider.id == id));
        (ordered, bound_was_eligible)
    };
    let (mut reader, mut writer) = stream.into_split();
    let disconnected = async {
        let mut buf = [0u8; 1024];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    };
    // One entry per completed upstream attempt, owned by the connection handler
    // and borrowed by the forwarding future: dropping an in-flight attempt keeps
    // every entry that already completed.
    let mut attempts: Vec<AttemptLog> = Vec::new();
    let forward = async {
        if wants_stream {
            if is_messages_path(path) {
                let mut native_writer = NativeHttpWriter::new(&mut writer);
                attempt_streaming_request(
                    &mut native_writer,
                    &ordered,
                    path,
                    &request.body,
                    requested.as_deref(),
                    &request.headers,
                    suppress_transport_failures,
                    probe.as_ref(),
                    &mut attempts,
                    &pool_fallback,
                    &config.settings,
                )
                .await
            } else {
                attempt_streaming_request(
                    &mut writer,
                    &ordered,
                    path,
                    &request.body,
                    requested.as_deref(),
                    &request.headers,
                    suppress_transport_failures,
                    probe.as_ref(),
                    &mut attempts,
                    &pool_fallback,
                    &config.settings,
                )
                .await
            }
        } else {
            let mut response = attempt_non_streaming_request(
                &ordered,
                path,
                &request.body,
                requested.as_deref(),
                &request.headers,
                suppress_transport_failures,
                probe.as_ref(),
                &mut attempts,
                &pool_fallback,
                &config.settings,
            )
            .await;
            let status = response.status;
            let mut capture = response.capture.take().unwrap_or_default();
            if is_pool && capture.result() == UsageResult::Success {
                if let Ok(value) = serde_json::from_slice::<Value>(&response.body) {
                    capture.served_tool_ids = pool_context_hold::tool_identities(path, &value);
                }
            }
            write_response(&mut writer, response).await?;
            Ok(ForwardCapture { status, ..capture })
        }
    };
    let outcome: Option<Result<ForwardCapture, String>> = tokio::select! {
        // Check disconnect first so a ready retry cannot start after EOF.
        // Dropping forwarding cancels upstream I/O and backoff without recording
        // the client cancellation as an upstream health failure.
        biased;
        _ = disconnected => None,
        result = forward => Some(result),
    };
    // A completed business outcome persists its buffered attempts with exactly
    // one terminal row (the successful, `ReturnToClient`, mid-stream-failure or
    // chronologically last exhausted attempt). A request without a completed
    // attempt writes the gateway's own synthetic failure terminal row instead.
    // A downstream cancellation or undeliverable response persists nothing: the
    // whole buffer, including completed attempts, is discarded. Logging is
    // best-effort and never changes the caller-visible response.
    match outcome {
        Some(Ok(capture)) if !capture.downstream_cancelled => {
            // Settle the binding once per request at this terminal outcome,
            // before the best-effort usage rows are persisted so the binding
            // is already settled when the caller observes the response. A
            // request that reached no upstream carries an empty provider id
            // and settles nothing; the lock is held for this call only. A
            // probe-only request never read a binding, so it settles none.
            if !probe_only && !is_pool {
                let served = capture.provider_id.trim();
                let served = if served.is_empty() {
                    None
                } else {
                    Some(served)
                };
                session_affinity()
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .settle(
                        session_id.as_deref().unwrap_or_default(),
                        requested.as_deref().unwrap_or_default(),
                        bound_was_eligible,
                        served,
                    );
            }
            // Publish, replace or close the same-turn continuation preference
            // only for a successfully delivered pool response. The admission
            // token was captured when this request entered the scope, so a newer
            // admitted turn in the same scope prevents a stale publication. A
            // failed or undeliverable request reaches neither branch and never
            // refreshes a completed hold.
            if capture.result() == UsageResult::Success {
                if let (Some(scope), Some(history), Some(admission), Some(identity)) = (
                    hold_scope.as_ref(),
                    hold_history.as_ref(),
                    hold_admission,
                    capture.pool_candidate.as_ref(),
                ) {
                    // A fallback delivery never becomes the next turn's preferred
                    // source. It closes any stale primary hold instead of
                    // publishing a fallback preference; a healthy regular success
                    // keeps the existing continuation behavior, even when its
                    // exact local model is also reachable through a fallback.
                    let record_tool_ids: &[String] = if capture.fallback_stage {
                        &[]
                    } else {
                        &capture.served_tool_ids
                    };
                    pool_context_hold::record(
                        scope,
                        admission,
                        history,
                        identity,
                        &capture.endpoint,
                        record_tool_ids,
                    );
                }
            }
            if attempts.is_empty() {
                // No upstream attempt completed, so the request's only row is
                // the gateway's own terminal row, like a no-candidate request.
                // It records the same all-unavailable diagnostic the
                // caller received instead of an empty error.
                let record = synthetic_terminal_row(
                    &config,
                    requested.as_deref().unwrap_or_default(),
                    capture.result(),
                    capture.status,
                    started.elapsed().as_millis().max(1) as u64,
                    capture.message.as_deref(),
                    reasoning_effort.clone(),
                    pool_resolved.clone(),
                );
                record_usage_log(&config, &record).await;
            } else {
                let terminal = attempts.len() - 1;
                record_request_usage_logs(
                    &config,
                    requested.as_deref(),
                    reasoning_effort.clone(),
                    &attempts,
                    terminal,
                )
                .await;
            }
        }
        // A downstream cancellation or an undeliverable response is an internal
        // transport lifecycle event, not a business outcome: discard the entire
        // buffered log set so this inbound request writes no row at all.
        _ => {}
    }
    Ok(())
}

/// Extract the requested reasoning effort level from request JSON body, if any.
/// Compatible with OpenAI `reasoning_effort`, OpenCode `reasoningEffort`, and nested `reasoning.effort`.
fn extract_reasoning_effort(body: &Value) -> Option<String> {
    if let Some(effort) = body.get("reasoning_effort").and_then(|v| v.as_str()) {
        let trimmed = effort.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Some(effort) = body.get("reasoningEffort").and_then(|v| v.as_str()) {
        let trimmed = effort.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Some(effort) = body
        .get("reasoning")
        .and_then(|v| v.get("effort"))
        .and_then(|v| v.as_str())
    {
        let trimmed = effort.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// The effective output cap of one pool request, used only to filter Anthropic
/// manual-budget mappings. Chat applies `max_completion_tokens`
/// precedence over `max_tokens`; Responses uses `max_output_tokens`; an omitted
/// cap keeps the Anthropic adapter's 4096 default. The field name travels with
/// the value so an incompatibility error can name the relevant field.
fn pool_output_cap(path: &str, body: &Value) -> PoolOutputCap {
    if path.split('?').next().unwrap_or(path) == "/v1/responses" {
        return PoolOutputCap {
            field: "max_output_tokens",
            tokens: body
                .get("max_output_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(4096),
        };
    }
    if let Some(tokens) = body.get("max_completion_tokens").and_then(Value::as_u64) {
        return PoolOutputCap {
            field: "max_completion_tokens",
            tokens,
        };
    }
    if let Some(tokens) = body.get("max_tokens").and_then(Value::as_u64) {
        return PoolOutputCap {
            field: "max_tokens",
            tokens,
        };
    }
    PoolOutputCap {
        field: "max_tokens",
        tokens: 4096,
    }
}
