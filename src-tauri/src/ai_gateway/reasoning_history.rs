//! Historical reasoning repair for Reflection/Beam Chat Completions upstreams.
//!
//! Reflection/Beam's Chat Completions API requires a non-empty
//! `reasoning_content` on every earlier assistant message. Clients that replay a
//! degraded turn with no recorded thinking would otherwise have the whole
//! request rejected. The provider accepts a non-empty placeholder for such
//! turns and treats whitespace-only values as missing, so a short visible marker
//! keeps the history acceptable without inventing reasoning text.

use serde_json::Value;

/// Marker used for assistant turns whose thinking was not recorded. Reflection
/// requires a non-whitespace value, so an empty or space-only placeholder would
/// still be rejected.
const MISSING_REASONING_PLACEHOLDER: &str = "[reasoning unavailable]";

/// Whether this upstream model is a Reflection/Beam model whose Chat
/// Completions API validates historical assistant reasoning.
pub(in crate::ai_gateway) fn model_requires_historical_reasoning(upstream_model: &str) -> bool {
    upstream_model
        .trim()
        .to_ascii_lowercase()
        .starts_with("reflection/")
}

/// Fill missing or blank `reasoning_content` on assistant messages in a Chat
/// Completions body.
///
/// Non-object bodies and bodies without a `messages` array are returned
/// unchanged. Existing non-empty strings are preserved exactly, and when no
/// message needs a placeholder the original bytes are returned untouched.
pub(in crate::ai_gateway) fn repair_historical_reasoning(body: &[u8]) -> Result<Vec<u8>, String> {
    let mut value: Value = serde_json::from_slice(body).map_err(|error| error.to_string())?;
    let repaired = match value.as_object_mut() {
        Some(object) => match object.get_mut("messages") {
            Some(Value::Array(messages)) => {
                let mut repaired = false;
                for message in messages {
                    let Some(message) = message.as_object_mut() else {
                        continue;
                    };
                    if message.get("role").and_then(Value::as_str) != Some("assistant") {
                        continue;
                    }
                    let needs_placeholder = match message.get("reasoning_content") {
                        None | Some(Value::Null) => true,
                        Some(Value::String(text)) => text.trim().is_empty(),
                        Some(_) => false,
                    };
                    if needs_placeholder {
                        message.insert(
                            "reasoning_content".to_string(),
                            Value::String(MISSING_REASONING_PLACEHOLDER.to_string()),
                        );
                        repaired = true;
                    }
                }
                repaired
            }
            _ => false,
        },
        None => false,
    };
    if !repaired {
        return Ok(body.to_vec());
    }
    serde_json::to_vec(&value).map_err(|error| error.to_string())
}
