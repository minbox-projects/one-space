use super::*;

/// The one gateway-generated error envelope (REQ-006/AC-010): `error.message`,
/// `error.type` and `error.code` are always present and non-empty, and
/// `error.param` always exists as `null`.
pub(super) fn error_envelope(message: impl Into<String>, error_type: &str, code: &str) -> Value {
    json!({
        "error": {
            "message": message.into(),
            "type": error_type,
            "code": code,
            "param": null,
        }
    })
}

pub(super) fn local_error_payload(
    path: &str,
    message: impl Into<String>,
    error_type: &str,
    code: &str,
) -> Value {
    if is_messages_path(path) {
        anthropic_error_payload(message, error_type)
    } else {
        error_envelope(message, error_type, code)
    }
}

/// Whether an upstream error body already uses the OpenAI standard shape: valid
/// JSON containing an `error` object. Such a body is passed through unchanged;
/// anything else is wrapped by the gateway (REQ-004/AC-007).
pub(super) fn is_standard_error_body(body: &[u8]) -> bool {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| value.get("error").map(Value::is_object))
        .unwrap_or(false)
}

/// Extract the structured upstream `error.type` string, if any. Used only to
/// recognize the model-scoped `FreeUsageLimitError` signal; an absent,
/// non-string or blank value yields `None`.
pub(super) fn extract_upstream_error_type(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.get("type"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

/// The standard envelope for a non-standard upstream 4xx body (REQ-004/AC-006).
/// The message names the upstream status and carries only the readable body
/// text, never credentials or request headers.
pub(super) fn upstream_error_payload(status: u16, body: &[u8]) -> Value {
    let readable = String::from_utf8_lossy(body);
    let trimmed = readable.trim();
    let message = if trimmed.is_empty() {
        format!("upstream returned HTTP {status} with an empty body")
    } else {
        format!("upstream returned HTTP {status}: {trimmed}")
    };
    error_envelope(message, "upstream_error", "upstream_error")
}

pub(super) fn all_unavailable_payload(message: impl Into<String>) -> Value {
    error_envelope(message, "server_error", "all_providers_unavailable")
}

pub(super) fn all_unavailable_payload_for_path(path: &str, message: impl Into<String>) -> Value {
    if is_messages_path(path) {
        anthropic_error_payload(message, "api_error")
    } else {
        all_unavailable_payload(message)
    }
}

pub(super) fn no_candidate_message(
    config: &GatewayConfig,
    requested: Option<&str>,
    protocol: UpstreamProtocol,
) -> String {
    let model = requested.unwrap_or("<none>");
    let endpoint = protocol.endpoint_path();
    // Enabled providers that serve the requested model through the opposite
    // endpoint, plus the diagnostics of every other enabled provider. The two
    // protocols are exclusive, so every mismatch names the same other endpoint.
    let mut mismatched_names: Vec<&str> = Vec::new();
    let mut served_via: Option<&'static str> = None;
    let mut others: Vec<String> = Vec::new();
    for provider in config.providers.iter().filter(|provider| provider.enabled) {
        match resolve_model_for_protocol(provider, requested, protocol) {
            ModelResolution::ProtocolMismatch(configured) => {
                let configured = configured.endpoint_path();
                served_via.get_or_insert(configured);
                mismatched_names.push(provider.name.as_str());
            }
            _ if provider.protocol != protocol => others.push(format!(
                "{} is configured for {}",
                provider.name,
                provider.protocol.endpoint_path()
            )),
            _ => others.push(format!("{} cannot serve model '{}'", provider.name, model)),
        }
    }
    // The model is reachable — just not over the endpoint this request used —
    // so lead with the remedy instead of claiming every provider is unavailable.
    if let Some(other) = served_via {
        let mut message = format!(
            "model '{model}' is served via {other} by {}, not via {endpoint}; call this model through the {other} endpoint [提示: 该模型仅支持 {other} 端点，请将客户端请求改用 {other} 调用]",
            mismatched_names.join(", ")
        );
        if !others.is_empty() {
            message.push_str(&format!(". Other enabled providers: {}", others.join("; ")));
        }
        return message;
    }
    if others.is_empty() {
        return format!(
            "no enabled provider can serve model '{model}' via {endpoint} (no enabled upstream providers)"
        );
    }
    format!(
        "no enabled provider can serve model '{model}' via {endpoint}; {}",
        others.join("; ")
    )
}

pub(in crate::ai_gateway) fn format_network_error_reason(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    let detail = if lower.contains("connection refused")
        || lower.contains("failed to connect")
        || lower.contains("unable to connect")
        || lower.contains("connection closed")
    {
        "connection refused / unreachable (请检查上游 Base URL 是否正确或网络代理设置)"
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "connection timed out (连接超时，请检查网络稳定性或代理延迟)"
    } else if lower.contains("dns")
        || lower.contains("resolve")
        || lower.contains("name resolution")
        || lower.contains("nodename nor servname provided")
    {
        "DNS resolution failed (无法解析上游域名，请检查 Base URL 拼写或 DNS 设置)"
    } else if lower.contains("certificate") || lower.contains("ssl") || lower.contains("tls") {
        "SSL/TLS certificate error (上游 SSL 证书验证失败)"
    } else {
        "connection failed (网络连接失败)"
    };
    format!("network error: {detail} ({error})")
}

pub(super) fn failure_reason(status: u16, body_parsed: bool, error_message: Option<&str>) -> String {
    let base = if body_parsed {
        format!("HTTP {status}")
    } else {
        format!("HTTP {status} with a non-JSON body")
    };
    let Some(msg) = error_message.map(str::trim).filter(|m| !m.is_empty()) else {
        return base;
    };
    if status == 429 {
        if is_quota_exceeded_message(error_message) {
            return format!("{base} (额度已用尽 / Quota Exceeded: {msg})");
        } else {
            return format!("{base} (请求频次超限 / Rate Limited: {msg})");
        }
    }
    format!("{base} ({msg})")
}

/// Recognize the upstream rejection raised when an earlier assistant turn is
/// sent without the `reasoning_content` that model requires. Bounded to the
/// provider's missing historical-reasoning parameter error, never any 400 or
/// any reasoning-related validation text.
pub(super) fn is_missing_historical_reasoning_rejection(status: u16, error_message: Option<&str>) -> bool {
    if status != 400 {
        return false;
    }
    let Some(message) = error_message.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    let lower = message.to_ascii_lowercase();
    lower.contains("missing required parameter")
        && lower.contains("messages[")
        && lower.contains("].reasoning_content")
        && lower.contains("earlier assistant message")
}

/// Classify one failed attempt, recognizing request-local model-pool skips
/// before the shared classifier runs: a retired upstream model's 410 Gone, a
/// model-capacity 413 Payload Too Large, and the historical-reasoning rejection.
///
/// These failures are specific to the pool source that produced them: a retired
/// or capacity-limited model says nothing about the upstream key or the mapping
/// row's health. `Transient` makes the pool skip that candidate and continue
/// with the next candidate or configured fallback without retrying it, backing
/// off, marking a key, counting mapping health or cooling the candidate. Every
/// ordinary non-pool attempt, including an ordinary 413, still reaches the
/// shared classifier and returns its client error unchanged.
pub(super) fn classify_attempt_failure(
    status: u16,
    network_error: bool,
    body_parsed: bool,
    error_message: Option<&str>,
    pool_candidate: bool,
) -> FailureClass {
    if pool_candidate
        && (matches!(status, 410 | 413)
            || is_missing_historical_reasoning_rejection(status, error_message))
    {
        return FailureClass::Transient;
    }
    classify_failure_with_message(status, network_error, body_parsed, error_message)
}
