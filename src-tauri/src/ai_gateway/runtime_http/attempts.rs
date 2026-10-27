use super::*;
use super::errors::{
    classify_attempt_failure, error_envelope, extract_upstream_error_type, failure_reason,
    is_standard_error_body, upstream_error_payload,
};
use super::http_io::json_response;


/// Per-request forwarding metadata used to write exactly one usage log row.
#[derive(Debug, Clone, Default)]
pub(in crate::ai_gateway) struct ForwardCapture {
    pub(in crate::ai_gateway) status: u16,
    pub(in crate::ai_gateway) provider_id: String,
    pub(in crate::ai_gateway) provider_name: String,
    pub(in crate::ai_gateway) upstream_model: String,
    pub(in crate::ai_gateway) protocol: UpstreamProtocol,
    pub(in crate::ai_gateway) usage: Option<UsageTokens>,
    /// No candidate could serve the request (including every candidate failing).
    pub(in crate::ai_gateway) all_unavailable: bool,
    /// The upstream stream failed after bytes had already reached the caller.
    pub(in crate::ai_gateway) upstream_error: bool,
    /// The downstream client went away mid-forward, so the request is neither
    /// a success nor an error and its whole log buffer is discarded.
    pub(in crate::ai_gateway) downstream_cancelled: bool,
    /// The gateway-generated terminal message of an all-unavailable 502 (the
    /// same text placed in the response's `error.message`), carried to the
    /// terminal log row when no upstream attempt completed. `None` for every
    /// path that stores its own per-attempt error text.
    pub(in crate::ai_gateway) message: Option<String>,
    /// Transport-only identity of the served model-pool candidate, present only
    /// on a completed attempt of a pool request. It feeds the same-turn
    /// continuation preference and never reaches a response, the usage log or a
    /// command payload.
    pub(in crate::ai_gateway) pool_candidate: Option<PoolCandidateIdentity>,
    /// The served candidate's resolved upstream base URL, so a changed endpoint
    /// invalidates a retained continuation preference.
    pub(in crate::ai_gateway) endpoint: String,
    /// Caller-protocol tool-call identities observed in the successfully
    /// delivered response, in arrival order. Transport-only: it feeds the
    /// same-turn continuation preference and never reaches a response, the usage
    /// log or a command payload.
    pub(in crate::ai_gateway) served_tool_ids: Vec<String>,
    /// Whether the delivered attempt came from the distinct fallback stage. Set
    /// by the transport path that actually attempted the fallback source, never
    /// inferred from configuration overlap, so an overlapping regular success
    /// keeps its primary continuation preference.
    pub(in crate::ai_gateway) fallback_stage: bool,
}

impl ForwardCapture {
    /// Final result classification: an HTTP 2xx is success only when the
    /// request was not all-unavailable or an upstream error.
    pub(in crate::ai_gateway) fn result(&self) -> UsageResult {
        if self.all_unavailable || self.upstream_error || self.status >= 400 || self.status == 0 {
            UsageResult::Failure
        } else {
            UsageResult::Success
        }
    }
}

/// Exact source identity actually attempted by one request: real provider and
/// original local/upstream pair plus effective protocol. The resolved reasoning
/// control is deliberately excluded because it is request-scoped.
pub(super) type AttemptedSourceKey = (String, String, String, &'static str);

pub(super) fn attempted_source_key(provider: &GatewayUpstreamProvider) -> Option<AttemptedSourceKey> {
    provider.pool_candidate.as_ref().map(|identity| {
        (
            identity.provider_id.trim().to_string(),
            identity.local_model.trim().to_string(),
            identity.upstream_model.trim().to_string(),
            identity.protocol.endpoint_path(),
        )
    })
}

pub(super) fn fallback_stage_key(stage: &PoolFallbackStage) -> Option<AttemptedSourceKey> {
    match stage {
        PoolFallbackStage::Ready(provider) => attempted_source_key(provider),
        PoolFallbackStage::Probe(candidate) => attempted_source_key(&candidate.provider),
    }
}

/// One completed upstream attempt of an inbound request, buffered by the
/// connection handler until the request ends.
///
/// A `status` of 0 means the attempt never received an upstream HTTP status
/// (network or stream failure); `result` is `Success` or `Failure` only, because
/// a downstream-cancelled or undeliverable request discards its whole buffer
/// without writing any row. `error_message` is the sanitized upstream error text
/// and must be extracted where the raw bytes are still available.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::ai_gateway) struct AttemptLog {
    pub(in crate::ai_gateway) provider_id: String,
    pub(in crate::ai_gateway) provider_name: String,
    pub(in crate::ai_gateway) upstream_model: String,
    pub(in crate::ai_gateway) protocol: UpstreamProtocol,
    pub(in crate::ai_gateway) status: u16,
    pub(in crate::ai_gateway) result: UsageResult,
    pub(in crate::ai_gateway) error_message: Option<String>,
    pub(in crate::ai_gateway) usage: Option<UsageTokens>,
    /// Whether the captured usage object decomposed into valid canonical tiers
    /// (`false` for a missing or invalid/conflicting usage object).
    pub(in crate::ai_gateway) usage_valid: bool,
    pub(in crate::ai_gateway) duration_ms: u64,
    /// Measured first meaningful upstream output latency in milliseconds,
    /// unrounded; `None` when unavailable.
    pub(in crate::ai_gateway) first_token_ms: Option<f64>,
    /// Average output tokens per second over the unrounded whole-attempt window;
    /// `None` when unavailable.
    pub(in crate::ai_gateway) avg_tps: Option<f64>,
    /// Pool-only resolved preference (`low|medium|high|xhigh|none|auto`); `None`
    /// for an ordinary attempt.
    pub(in crate::ai_gateway) resolved_reasoning_effort: Option<String>,
    /// Pool-only allowlisted native-control snapshot actually sent upstream;
    /// `None` for an ordinary attempt.
    pub(in crate::ai_gateway) native_reasoning: Option<NativeReasoningSnapshot>,
}

/// Buffer one completed upstream attempt with its own elapsed time. The parsed
/// canonical usage carries both the persisted tiers and their validity. The
/// independent stream-timing measurements are supplied only by the streaming
/// path (fresh per attempt); every non-streaming or no-output attempt passes
/// `None`.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_attempt_log(
    provider: &GatewayUpstreamProvider,
    protocol: UpstreamProtocol,
    upstream_model: &str,
    started: Instant,
    status: u16,
    result: UsageResult,
    error_message: Option<String>,
    usage: Option<CanonicalUsage>,
    first_token_ms: Option<f64>,
    avg_tps: Option<f64>,
) -> AttemptLog {
    // Pool-only attempt evidence derived from the same resolved control that
    // shaped the upstream body; an ordinary attempt records none.
    let (resolved_reasoning_effort, native_reasoning) = match provider
        .pool_candidate
        .as_ref()
        .and_then(|identity| identity.reasoning.as_ref())
    {
        Some(native) => {
            let protocol = provider
                .pool_candidate
                .as_ref()
                .map(|identity| identity.protocol)
                .unwrap_or(protocol);
            (
                Some(native.resolved.clone()),
                Some(NativeReasoningSnapshot {
                    protocol,
                    mode: native.mode().to_string(),
                    parameters: native.wire_parameters(),
                }),
            )
        }
        None => (None, None),
    };
    AttemptLog {
        provider_id: provider.id.clone(),
        provider_name: provider.name.clone(),
        upstream_model: upstream_model.to_string(),
        protocol,
        status,
        result,
        error_message,
        usage: usage.map(|canonical| canonical.tokens),
        usage_valid: usage.is_some_and(|canonical| canonical.valid),
        duration_ms: started.elapsed().as_millis().max(1) as u64,
        first_token_ms,
        avg_tps,
        resolved_reasoning_effort,
        native_reasoning,
    }
}

/// The exact upstream model and protocol for one attempt.
///
/// A prepared model-pool candidate carries its full stored identity, so its real
/// upstream model and declared protocol are used directly; this is what lets a
/// pool candidate cross the two OpenAI endpoints without changing the ordinary
/// `can_convert_from` selection boundary. An ordinary provider keeps its
/// existing resolution untouched.
pub(super) fn attempt_target(
    provider: &GatewayUpstreamProvider,
    requested: Option<&str>,
    protocol: UpstreamProtocol,
) -> Option<(String, UpstreamProtocol)> {
    if let Some(identity) = provider.pool_candidate.as_ref() {
        return Some((identity.upstream_model.clone(), identity.protocol));
    }
    match resolve_model_for_protocol(provider, requested, protocol) {
        ModelResolution::Serve(model) => {
            let upstream =
                resolve_upstream_protocol(provider, requested, protocol).unwrap_or(protocol);
            Some((model, upstream))
        }
        ModelResolution::ProtocolMismatch(_) | ModelResolution::NoMatch => None,
    }
}

pub(super) async fn attempt_candidate(
    provider: &GatewayUpstreamProvider,
    path: &str,
    body: &[u8],
    model: &str,
    requested: Option<&str>,
    upstream_protocol: UpstreamProtocol,
    client_headers: &HashMap<String, String>,
    settings: &GatewaySettings,
) -> (AttemptResult, AttemptLog) {
    let started = Instant::now();
    match forward_non_streaming_with_protocol(
        provider,
        path,
        body,
        model,
        client_headers,
        upstream_protocol,
        settings,
    )
    .await
    {
        Ok(response) => {
            // Usage is only meaningful for a successful 2xx upstream response;
            // an error body that happens to carry `usage` must never be billed.
            let usage = if (200..300).contains(&response.status) {
                parse_usage_from_response(&response.body)
            } else {
                None
            };
            // A served 2xx is a success; every other body is a failed attempt
            // whose error text must be extracted while the raw bytes are still
            // available, because the retryable path below keeps only a status
            // reason.
            let served = response.status < 400 && response.parsed;
            let error_message = if served {
                None
            } else {
                sanitize_provider_error_text(
                    &extract_upstream_error_text(&response.body).unwrap_or_default(),
                    provider,
                )
            };
            let log = build_attempt_log(
                provider,
                upstream_protocol,
                model,
                started,
                response.status,
                if served {
                    UsageResult::Success
                } else {
                    UsageResult::Failure
                },
                error_message.clone(),
                usage,
                None,
                None,
            );
            let capture = ForwardCapture {
                status: response.status,
                provider_id: provider.id.clone(),
                provider_name: provider.name.clone(),
                upstream_model: model.to_string(),
                protocol: upstream_protocol,
                usage: usage.map(|canonical| canonical.tokens),
                pool_candidate: provider.pool_candidate.clone(),
                endpoint: provider.base_url.clone(),
                ..Default::default()
            };
            if served {
                let convert = provider.pool_candidate.is_some()
                    || (upstream_protocol == UpstreamProtocol::AnthropicMessages
                        && is_conversion_path(path));
                let body = if convert {
                    let converted = if upstream_protocol == UpstreamProtocol::AnthropicMessages {
                        convert_response(path, &response.body, requested.unwrap_or(model))
                    } else {
                        openai_conversion::convert_response(
                            path,
                            upstream_protocol,
                            &response.body,
                            requested.unwrap_or(model),
                        )
                    };
                    match converted {
                        Ok(body) => body,
                        Err(error) => {
                            let log = build_attempt_log(
                                provider,
                                upstream_protocol,
                                model,
                                started,
                                502,
                                UsageResult::Failure,
                                Some(error.clone()),
                                usage,
                                None,
                                None,
                            );
                            return (
                                AttemptResult::ReturnToClient(json_response(
                                    502,
                                    error_envelope(error, "server_error", "conversion_error"),
                                )),
                                log,
                            );
                        }
                    }
                } else {
                    response.body
                };
                return (
                    AttemptResult::Success(HttpResponse {
                        status: response.status,
                        content_type: "application/json",
                        body,
                        capture: Some(capture),
                    }),
                    log,
                );
            }
            // Quota-exhausted 429s count toward mapping health (Retryable) but
            // never requeue the same provider; plain rate-limit 429s stay
            // Transient and are still retried.
            let class = classify_attempt_failure(
                response.status,
                false,
                response.parsed,
                error_message.as_deref(),
                provider.pool_candidate.is_some(),
            );
            if class == FailureClass::ReturnToClient {
                // A standard upstream error body stays byte-for-byte; a
                // non-standard one keeps the status but is wrapped so clients
                // can read `error.message`.
                let standard = is_standard_error_body(&response.body);
                let body = if is_messages_path(path) || standard {
                    response.body
                } else {
                    serde_json::to_vec(&upstream_error_payload(response.status, &response.body))
                        .unwrap_or_else(|_| b"{}".to_vec())
                };
                return (
                    AttemptResult::ReturnToClient(HttpResponse {
                        status: response.status,
                        content_type: "application/json",
                        body,
                        capture: Some(capture),
                    }),
                    log,
                );
            }
            let key_failure = key_failure_kind(response.status, error_message.as_deref());
            // Only a prepared model-pool candidate can be candidate-scoped, and
            // the classifier never applies to an ordinary attempt.
            let error_type = extract_upstream_error_type(&response.body);
            let pool_scope = provider.pool_candidate.is_some()
                && pool_candidate_scope_failure(
                    response.status,
                    error_message.as_deref(),
                    error_type.as_deref(),
                    false,
                );
            (
                AttemptResult::Failure {
                    class,
                    retryable: is_retryable_with_message(
                        class,
                        response.status,
                        error_message.as_deref(),
                    ),
                    transport: false,
                    reason: failure_reason(
                        response.status,
                        response.parsed,
                        error_message.as_deref(),
                    ),
                    retry_delay: retry_header_delay(&response.headers),
                    bare_rotation: response.status == 429 && key_failure.is_none(),
                    key_failure,
                    pool_scope,
                },
                log,
            )
        }
        Err(error) => {
            let reason = format_network_error_reason(&error);
            let log = build_attempt_log(
                provider,
                upstream_protocol,
                model,
                started,
                0,
                UsageResult::Failure,
                sanitize_provider_error_text(&reason, provider),
                None,
                None,
                None,
            );
            (
                AttemptResult::Failure {
                    class: FailureClass::Retryable,
                    retryable: true,
                    transport: true,
                    reason,
                    retry_delay: None,
                    key_failure: None,
                    bare_rotation: false,
                    pool_scope: false,
                },
                log,
            )
        }
    }
}

/// A 2xx streaming response is only a success if it actually looks like a
/// Server-Sent Events stream. Checked before any byte reaches the caller so a
/// plain non-SSE body stays a retryable failure and can still switch.
pub(super) fn is_sse_response_chunk(content_type: &str, first_chunk: &[u8]) -> bool {
    if !content_type
        .to_ascii_lowercase()
        .contains("text/event-stream")
    {
        return false;
    }
    if first_chunk.is_empty() {
        return true;
    }
    let Ok(text) = std::str::from_utf8(first_chunk) else {
        return false;
    };
    let trimmed = text.trim_start_matches(['\r', '\n', ' ', '\t']);
    trimmed.starts_with("data:")
        || trimmed.starts_with("event:")
        || trimmed.starts_with("id:")
        || trimmed.starts_with("retry:")
        || trimmed.starts_with(':')
}
