//! Responses <-> Chat Completions response conversion.

use super::super::UpstreamProtocol;
use super::protocol::client_protocol;
use serde_json::{json, Map, Value};

// ---------------------------------------------------------------------------
// Response conversion
// ---------------------------------------------------------------------------

fn usage_input(usage: &Value) -> u64 {
    usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn usage_output(usage: &Value) -> u64 {
    usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn usage_total(usage: &Value) -> u64 {
    usage
        .get("total_tokens")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| usage_input(usage).saturating_add(usage_output(usage)))
}

fn cached_tokens(usage: &Value) -> Option<u64> {
    usage
        .get("prompt_tokens_details")
        .and_then(|details| details.get("cached_tokens"))
        .or_else(|| {
            usage
                .get("input_tokens_details")
                .and_then(|details| details.get("cached_tokens"))
        })
        .and_then(Value::as_u64)
}

/// Convert a non-streaming pool response to the client's OpenAI protocol and
/// rewrite the public pool alias. `upstream` is the real source protocol.
pub(in crate::ai_gateway) fn convert_response(
    path: &str,
    upstream: UpstreamProtocol,
    body: &[u8],
    model: &str,
) -> Result<Vec<u8>, String> {
    let mut value: Value = serde_json::from_slice(body).map_err(|error| error.to_string())?;
    let Some(object) = value.as_object_mut() else {
        return Err("upstream response must be a JSON object".to_string());
    };
    let client = client_protocol(path);
    if client == Some(upstream) {
        object.insert("model".to_string(), Value::String(model.to_string()));
        if upstream == UpstreamProtocol::Responses {
            if let Some(response) = object.get_mut("response").and_then(Value::as_object_mut) {
                response.insert("model".to_string(), Value::String(model.to_string()));
            }
        }
        return serde_json::to_vec(&value).map_err(|error| error.to_string());
    }
    let converted = match (upstream, client) {
        (UpstreamProtocol::Responses, Some(UpstreamProtocol::ChatCompletions)) => {
            responses_response_to_chat(object, model)?
        }
        (UpstreamProtocol::ChatCompletions, Some(UpstreamProtocol::Responses)) => {
            chat_response_to_responses(object, model)?
        }
        _ => return Err("unsupported OpenAI response conversion direction".to_string()),
    };
    serde_json::to_vec(&converted).map_err(|error| error.to_string())
}

fn responses_response_to_chat(object: &Map<String, Value>, model: &str) -> Result<Value, String> {
    // A 2xx body that is actually an error/failed response must never be
    // fabricated into a completed Chat answer.
    if object.get("error").is_some_and(Value::is_object)
        || object.get("type").and_then(Value::as_str) == Some("error")
    {
        return Err("upstream Responses body is an error".to_string());
    }
    let status = object
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("completed");
    if status == "failed" {
        return Err("upstream Responses body failed".to_string());
    }
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("chatcmpl-pool");
    let mut text = String::new();
    let mut tool_calls: Vec<Value> = Vec::new();
    for item in object
        .get("output")
        .and_then(Value::as_array)
        .map_or(&[][..], |output| output.as_slice())
    {
        match item.get("type").and_then(Value::as_str) {
            Some("message") => {
                for content in item
                    .get("content")
                    .and_then(Value::as_array)
                    .map_or(&[][..], |content| content.as_slice())
                {
                    if content.get("type").and_then(Value::as_str) == Some("output_text") {
                        if let Some(value) = content.get("text").and_then(Value::as_str) {
                            text.push_str(value);
                        }
                    }
                }
            }
            Some("function_call") => {
                tool_calls.push(json!({
                    "id": item.get("call_id").or_else(|| item.get("id")).and_then(Value::as_str).unwrap_or_default(),
                    "type": "function",
                    "function": {
                        "name": item.get("name").and_then(Value::as_str).unwrap_or_default(),
                        "arguments": item.get("arguments").and_then(Value::as_str).unwrap_or_default(),
                    },
                }));
            }
            _ => {}
        }
    }
    let usage = object.get("usage").cloned().unwrap_or_else(|| json!({}));
    let mut chat_usage = json!({
        "prompt_tokens": usage_input(&usage),
        "completion_tokens": usage_output(&usage),
        "total_tokens": usage_total(&usage),
    });
    if let Some(cached) = cached_tokens(&usage) {
        chat_usage["prompt_tokens_details"] = json!({"cached_tokens": cached});
    }
    let has_tools = !tool_calls.is_empty();
    let finish_reason = if has_tools {
        "tool_calls"
    } else if status == "incomplete" {
        "length"
    } else {
        "stop"
    };
    Ok(json!({
        "id": id,
        "object": "chat.completion",
        "model": model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": if text.is_empty() { Value::Null } else { Value::String(text) },
                "tool_calls": if has_tools { Value::Array(tool_calls) } else { Value::Null },
            },
            "finish_reason": finish_reason,
        }],
        "usage": chat_usage,
    }))
}

fn chat_response_to_responses(object: &Map<String, Value>, model: &str) -> Result<Value, String> {
    // A 2xx body that is actually an error must never be fabricated into a
    // completed Responses answer, and a Chat completion always carries choices.
    if object.get("error").is_some_and(Value::is_object) {
        return Err("upstream Chat body is an error".to_string());
    }
    let choices = object
        .get("choices")
        .and_then(Value::as_array)
        .filter(|choices| !choices.is_empty())
        .ok_or_else(|| "upstream Chat body has no choices".to_string())?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("resp_pool");
    let choice = choices.first();
    let message = choice.and_then(|choice| choice.get("message"));
    let mut output: Vec<Value> = Vec::new();
    let text = message
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !text.is_empty() {
        output.push(json!({
            "type": "message",
            "id": format!("{id}_message"),
            "role": "assistant",
            "status": "completed",
            "content": [{"type": "output_text", "text": text, "annotations": []}],
        }));
    }
    for call in message
        .and_then(|message| message.get("tool_calls"))
        .and_then(Value::as_array)
        .map_or(&[][..], |calls| calls.as_slice())
    {
        output.push(json!({
            "type": "function_call",
            "id": call.get("id").and_then(Value::as_str).unwrap_or_default(),
            "call_id": call.get("id").and_then(Value::as_str).unwrap_or_default(),
            "name": call.get("function").and_then(|function| function.get("name")).and_then(Value::as_str).unwrap_or_default(),
            "arguments": call.get("function").and_then(|function| function.get("arguments")).and_then(Value::as_str).unwrap_or_default(),
            "status": "completed",
        }));
    }
    if output.is_empty() {
        output.push(json!({
            "type": "message",
            "id": format!("{id}_message"),
            "role": "assistant",
            "status": "completed",
            "content": [],
        }));
    }
    let usage = object.get("usage").cloned().unwrap_or_else(|| json!({}));
    let finish = choice
        .and_then(|choice| choice.get("finish_reason"))
        .and_then(Value::as_str)
        .unwrap_or("stop");
    let status = match finish {
        "length" | "max_tokens" | "content_filter" => "incomplete",
        _ => "completed",
    };
    Ok(json!({
        "id": id,
        "object": "response",
        "model": model,
        "status": status,
        "output": output,
        "usage": {
            "input_tokens": usage_input(&usage),
            "output_tokens": usage_output(&usage),
            "total_tokens": usage_total(&usage),
        },
    }))
}
