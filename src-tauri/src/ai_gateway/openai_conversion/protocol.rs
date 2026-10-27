//! Canonical OpenAI endpoint paths and inbound client protocol detection.

use super::super::UpstreamProtocol;
use serde_json::Value;

/// Canonical path without its query string.
fn clean(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

/// Whether `path` is one of the two OpenAI-compatible gateway endpoints.
pub(in crate::ai_gateway) fn is_openai_path(path: &str) -> bool {
    matches!(clean(path), "/v1/chat/completions" | "/v1/responses")
}

/// The protocol an inbound canonical OpenAI path speaks, if any.
pub(in crate::ai_gateway) fn client_protocol(path: &str) -> Option<UpstreamProtocol> {
    match clean(path) {
        "/v1/chat/completions" => Some(UpstreamProtocol::ChatCompletions),
        "/v1/responses" => Some(UpstreamProtocol::Responses),
        _ => None,
    }
}

/// The canonical upstream path for one OpenAI protocol.
pub(in crate::ai_gateway) fn endpoint_path(protocol: UpstreamProtocol) -> &'static str {
    match protocol {
        UpstreamProtocol::ChatCompletions => "/v1/chat/completions",
        UpstreamProtocol::Responses => "/v1/responses",
        UpstreamProtocol::AnthropicMessages => "/v1/messages",
    }
}

/// Whether a Chat request supplies `stream_options.include_usage: true`. Used so
/// the converted output honors the caller's terminal-usage request without ever
/// injecting the flag upstream.
pub(in crate::ai_gateway) fn include_usage_requested(body: &Value) -> bool {
    body.get("stream_options")
        .and_then(|options| options.get("include_usage"))
        .and_then(Value::as_bool)
        == Some(true)
}
