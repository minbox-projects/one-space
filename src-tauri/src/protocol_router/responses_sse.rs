use super::{CatalogModel, HttpResponse};
use serde_json::{json, Value};

pub(in crate::protocol_router) fn json_response(status: u16, body: Value) -> HttpResponse {
    let payload = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
    HttpResponse {
        status,
        content_type: "application/json",
        body: payload,
    }
}

pub(in crate::protocol_router) fn sse_response(status: u16, body: Vec<u8>) -> HttpResponse {
    HttpResponse {
        status,
        content_type: "text/event-stream",
        body,
    }
}

pub(in crate::protocol_router) fn http_response_bytes(response: HttpResponse) -> Vec<u8> {
    let header = format!(
        "HTTP/1.1 {} {}\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        response.status,
        reason_for_status(response.status),
        response.content_type,
        response.body.len()
    );
    [header.into_bytes(), response.body].concat()
}

pub(in crate::protocol_router) fn reason_for_status(status: u16) -> &'static str {
    match status {
        200..=299 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        502 => "Bad Gateway",
        _ => "Internal Server Error",
    }
}

pub(in crate::protocol_router) fn openai_sse_to_anthropic_sse(
    input: &[u8],
    model: &str,
) -> Vec<u8> {
    let raw = String::from_utf8_lossy(input);
    let message_id = format!("msg_{}", uuid::Uuid::new_v4().simple());
    let mut out = String::new();
    let mut tool_calls: Vec<StreamToolCall> = Vec::new();
    out.push_str(&sse_event(
        "message_start",
        json!({
            "type": "message_start",
            "message": {
                "id": message_id,
                "type": "message",
                "role": "assistant",
                "model": model,
                "content": [],
                "stop_reason": Value::Null,
                "stop_sequence": Value::Null,
                "usage": { "input_tokens": 0, "output_tokens": 0 }
            }
        }),
    ));
    out.push_str(&sse_event(
        "content_block_start",
        json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": { "type": "text", "text": "" }
        }),
    ));

    for line in raw.lines() {
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
            break;
        }
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        if let Some(text) = openai_stream_text_delta(&value) {
            if !text.is_empty() {
                out.push_str(&sse_event(
                    "content_block_delta",
                    json!({
                        "type": "content_block_delta",
                        "index": 0,
                        "delta": { "type": "text_delta", "text": text }
                    }),
                ));
            }
        }
        collect_openai_stream_tool_calls(&value, &mut tool_calls);
    }
    out.push_str(&sse_event(
        "content_block_stop",
        json!({ "type": "content_block_stop", "index": 0 }),
    ));

    for (offset, tool_call) in tool_calls.iter().enumerate() {
        let index = offset + 1;
        out.push_str(&sse_event(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": index,
                "content_block": {
                    "type": "tool_use",
                    "id": if tool_call.id.is_empty() { format!("toolu_{}", uuid::Uuid::new_v4().simple()) } else { tool_call.id.clone() },
                    "name": if tool_call.name.is_empty() { "tool".to_string() } else { tool_call.name.clone() },
                    "input": {}
                }
            }),
        ));
        if !tool_call.arguments.is_empty() {
            out.push_str(&sse_event(
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": index,
                    "delta": {
                        "type": "input_json_delta",
                        "partial_json": tool_call.arguments
                    }
                }),
            ));
        }
        out.push_str(&sse_event(
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": index }),
        ));
    }

    let stop_reason = if tool_calls.is_empty() {
        "end_turn"
    } else {
        "tool_use"
    };
    out.push_str(&sse_event(
        "message_delta",
        json!({
            "type": "message_delta",
            "delta": { "stop_reason": stop_reason, "stop_sequence": Value::Null },
            "usage": { "output_tokens": 0 }
        }),
    ));
    out.push_str(&sse_event(
        "message_stop",
        json!({ "type": "message_stop" }),
    ));
    out.into_bytes()
}

pub(in crate::protocol_router) fn sse_event(event: &str, data: Value) -> String {
    format!("event: {event}\ndata: {}\n\n", data)
}

pub(in crate::protocol_router) fn openai_stream_text_delta(value: &Value) -> Option<String> {
    value
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("delta"))
        .and_then(|delta| delta.get("content"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            value
                .get("response")
                .and_then(|response| response.get("output_text"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            value
                .get("delta")
                .and_then(|delta| delta.get("text").or_else(|| delta.get("content")))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            value
                .get("delta")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
}

#[derive(Default)]
pub(in crate::protocol_router) struct StreamToolCall {
    id: String,
    name: String,
    arguments: String,
}

pub(in crate::protocol_router) fn collect_openai_stream_tool_calls(
    value: &Value,
    tool_calls: &mut Vec<StreamToolCall>,
) {
    let Some(calls) = value
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("delta"))
        .and_then(|delta| delta.get("tool_calls"))
        .and_then(|v| v.as_array())
    else {
        return;
    };
    for call in calls {
        let index = call.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        while tool_calls.len() <= index {
            tool_calls.push(StreamToolCall::default());
        }
        let target = &mut tool_calls[index];
        if let Some(id) = call.get("id").and_then(|v| v.as_str()) {
            target.id = id.to_string();
        }
        if let Some(function) = call.get("function") {
            if let Some(name) = function.get("name").and_then(|v| v.as_str()) {
                target.name = name.to_string();
            }
            if let Some(arguments) = function.get("arguments").and_then(|v| v.as_str()) {
                target.arguments.push_str(arguments);
            }
        }
    }
}

#[allow(dead_code)]
pub(crate) fn parse_openai_models_catalog(
    value: &Value,
    prefix: Option<&str>,
) -> Result<Vec<CatalogModel>, String> {
    let data = value
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "models response must contain data array".to_string())?;
    let prefix = prefix.unwrap_or("").trim();
    let mut models = Vec::new();
    for item in data {
        let Some(id) = item.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let id = if prefix.is_empty() || id.starts_with(prefix) {
            id.to_string()
        } else {
            format!("{prefix}{id}")
        };
        models.push(CatalogModel {
            id,
            object: item
                .get("object")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            created: item.get("created").and_then(|v| v.as_u64()),
            owned_by: item
                .get("owned_by")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        });
    }
    Ok(models)
}
