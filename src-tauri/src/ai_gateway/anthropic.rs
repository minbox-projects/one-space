//! Native Anthropic Messages protocol helpers.

use serde_json::{json, Value};

pub(in crate::ai_gateway) const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";

pub(in crate::ai_gateway) fn is_messages_path(path: &str) -> bool {
    path.split('?').next().unwrap_or(path) == "/v1/messages"
}

pub(in crate::ai_gateway) fn error_payload(message: impl Into<String>, error_type: &str) -> Value {
    json!({
        "type": "error",
        "error": {
            "type": error_type,
            "message": message.into(),
        }
    })
}

pub(in crate::ai_gateway) fn stream_error_event(
    message: impl Into<String>,
    error_type: &str,
) -> Vec<u8> {
    let payload = error_payload(message, error_type);
    format!("event: error\ndata: {payload}\n\n").into_bytes()
}
