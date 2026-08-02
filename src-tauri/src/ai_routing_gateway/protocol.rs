use serde_json::{json, Map, Value};
use std::collections::HashSet;

use super::types::UpstreamProtocol;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProtocolError {
    pub(crate) code: &'static str,
    pub(crate) message: &'static str,
}

#[derive(Debug, Clone)]
pub(crate) struct SseConversionState {
    source: UpstreamProtocol,
    target: UpstreamProtocol,
    model: String,
    request_id: String,
    response_id: Option<String>,
    message_item_id: String,
    tools: Vec<StreamToolState>,
    finish_reason: Option<String>,
    usage: Option<Value>,
    completion_emitted: bool,
}

#[derive(Debug, Clone)]
struct StreamToolState {
    key: String,
    index: usize,
    id: String,
    name: String,
    arguments: String,
    output_item_emitted: bool,
}

impl SseConversionState {
    pub(crate) fn new(
        source: UpstreamProtocol,
        target: UpstreamProtocol,
        model: &str,
        request_id: &str,
    ) -> Self {
        Self {
            source,
            target,
            model: model.to_owned(),
            request_id: request_id.to_owned(),
            response_id: None,
            message_item_id: format!("msg_{}", request_id.trim_start_matches("req_")),
            tools: Vec::new(),
            finish_reason: None,
            usage: None,
            completion_emitted: false,
        }
    }
}

pub(crate) fn error_envelope(code: &str, message: &str) -> Value {
    json!({
        "error": {
            "message": message,
            "type": "invalid_request_error",
            "param": Value::Null,
            "code": code
        }
    })
}

pub(crate) fn convert_request(
    source: UpstreamProtocol,
    target: UpstreamProtocol,
    input: &Value,
    upstream_model: &str,
) -> Result<Value, ProtocolError> {
    let object = input.as_object().ok_or_else(invalid_request)?;
    let allowed = match source {
        UpstreamProtocol::Responses => responses_request_fields(),
        UpstreamProtocol::ChatCompletions => chat_request_fields(),
    };
    reject_unknown(object, &allowed)?;
    if input.get("model").and_then(Value::as_str).is_none() {
        return Err(invalid_request());
    }
    if source == target {
        let mut output = object.clone();
        output.insert("model".into(), Value::String(upstream_model.into()));
        return Ok(Value::Object(output));
    }
    if input.get("stream_options").is_some() {
        return Err(lossless());
    }
    match (source, target) {
        (UpstreamProtocol::ChatCompletions, UpstreamProtocol::Responses) => {
            chat_to_responses_request(input, upstream_model)
        }
        (UpstreamProtocol::Responses, UpstreamProtocol::ChatCompletions) => {
            responses_to_chat_request(input, upstream_model)
        }
        _ => unreachable!(),
    }
}

pub(crate) fn convert_response(
    source: UpstreamProtocol,
    target: UpstreamProtocol,
    value: &Value,
    public_model: &str,
) -> Result<Value, ProtocolError> {
    if source == target {
        let mut output = value.clone();
        if let Some(object) = output.as_object_mut() {
            object.insert("model".into(), Value::String(public_model.into()));
        }
        return Ok(output);
    }
    match (source, target) {
        (UpstreamProtocol::Responses, UpstreamProtocol::ChatCompletions) => {
            responses_to_chat_response(value, public_model)
        }
        (UpstreamProtocol::ChatCompletions, UpstreamProtocol::Responses) => {
            chat_to_responses_response(value, public_model)
        }
        _ => unreachable!(),
    }
}

pub(crate) fn convert_sse(
    source: UpstreamProtocol,
    target: UpstreamProtocol,
    bytes: &[u8],
    public_model: &str,
) -> Result<Vec<u8>, ProtocolError> {
    if source == target {
        return Ok(bytes.to_vec());
    }
    let mut state = SseConversionState::new(source, target, public_model, "compat");
    convert_sse_with_state(&mut state, bytes, true)
}

pub(crate) fn convert_sse_with_state(
    state: &mut SseConversionState,
    bytes: &[u8],
    finished: bool,
) -> Result<Vec<u8>, ProtocolError> {
    if state.source == state.target {
        return Ok(bytes.to_vec());
    }
    let raw = std::str::from_utf8(bytes).map_err(|_| invalid_request())?;
    let mut output = String::new();
    for block in raw.split("\n\n") {
        let data_lines = block
            .lines()
            .filter_map(|line| line.strip_prefix("data:").map(str::trim))
            .collect::<Vec<_>>();
        if data_lines.is_empty() {
            continue;
        }
        let data = data_lines.join("\n");
        if data == "[DONE]" {
            for event in finish_sse_state(state)? {
                append_sse_event(&mut output, &event);
            }
            output.push_str("data: [DONE]\n\n");
            continue;
        }
        let value: Value = serde_json::from_str(&data).map_err(|_| invalid_request())?;
        for event in convert_stream_event_with_state(state, &value)? {
            append_sse_event(&mut output, &event);
        }
    }
    if finished && !raw.contains("[DONE]") {
        for event in finish_sse_state(state)? {
            append_sse_event(&mut output, &event);
        }
    }
    Ok(output.into_bytes())
}

fn append_sse_event(output: &mut String, event: &Value) {
    output.push_str("data: ");
    output.push_str(&event.to_string());
    output.push_str("\n\n");
}

fn chat_to_responses_request(input: &Value, model: &str) -> Result<Value, ProtocolError> {
    let mut output = Map::new();
    output.insert("model".into(), Value::String(model.into()));
    output.insert(
        "input".into(),
        chat_messages_to_responses(input.get("messages").ok_or_else(invalid_request)?)?,
    );
    copy(input, &mut output, "stream", "stream");
    copy(input, &mut output, "temperature", "temperature");
    copy(input, &mut output, "max_tokens", "max_output_tokens");
    if let Some(tools) = input.get("tools") {
        output.insert("tools".into(), chat_tools_to_responses(tools)?);
    }
    if let Some(choice) = input.get("tool_choice") {
        output.insert("tool_choice".into(), chat_tool_choice_to_responses(choice)?);
    }
    copy(
        input,
        &mut output,
        "parallel_tool_calls",
        "parallel_tool_calls",
    );
    if let Some(effort) = input.get("reasoning_effort") {
        output.insert("reasoning".into(), json!({ "effort": effort }));
    }
    copy_common_request_fields(input, &mut output);
    Ok(Value::Object(output))
}

fn responses_to_chat_request(input: &Value, model: &str) -> Result<Value, ProtocolError> {
    let input_value = input.get("input").ok_or_else(invalid_request)?;
    let messages = responses_input_to_chat(input_value)?;
    let mut output = Map::new();
    output.insert("model".into(), Value::String(model.into()));
    output.insert("messages".into(), messages);
    copy(input, &mut output, "stream", "stream");
    copy(input, &mut output, "temperature", "temperature");
    copy(input, &mut output, "max_output_tokens", "max_tokens");
    if let Some(tools) = input.get("tools") {
        output.insert("tools".into(), responses_tools_to_chat(tools)?);
    }
    if let Some(choice) = input.get("tool_choice") {
        output.insert("tool_choice".into(), responses_tool_choice_to_chat(choice)?);
    }
    copy(
        input,
        &mut output,
        "parallel_tool_calls",
        "parallel_tool_calls",
    );
    if let Some(reasoning) = input.get("reasoning") {
        let reasoning = reasoning.as_object().ok_or_else(lossless)?;
        if reasoning.keys().any(|key| key != "effort") {
            return Err(lossless());
        }
        let effort = reasoning.get("effort").cloned().ok_or_else(lossless)?;
        output.insert("reasoning_effort".into(), effort);
    }
    copy_common_request_fields(input, &mut output);
    Ok(Value::Object(output))
}

fn copy_common_request_fields(input: &Value, output: &mut Map<String, Value>) {
    for field in ["metadata", "store", "user"] {
        copy(input, output, field, field);
    }
}

fn chat_messages_to_responses(input: &Value) -> Result<Value, ProtocolError> {
    let messages = input.as_array().ok_or_else(lossless)?;
    let mut output = Vec::new();
    for message in messages {
        let object = message.as_object().ok_or_else(lossless)?;
        let role = object
            .get("role")
            .and_then(Value::as_str)
            .ok_or_else(lossless)?;
        match role {
            "user" | "system" | "developer" | "assistant" => {
                let allowed = ["role", "content", "tool_calls"];
                reject_object_keys(object, &allowed)?;
                let mut converted = Map::new();
                converted.insert("type".into(), Value::String("message".into()));
                converted.insert("role".into(), Value::String(role.into()));
                if let Some(content) = object.get("content") {
                    converted.insert(
                        "content".into(),
                        chat_content_to_responses(content, role == "assistant")?,
                    );
                }
                if role == "assistant" {
                    let tool_calls = object.get("tool_calls");
                    if let Some(tool_calls) = tool_calls {
                        if !converted.contains_key("content") && !tool_calls.is_array() {
                            return Err(lossless());
                        }
                        if let Some(content) = converted.get("content") {
                            if content.is_null() {
                                converted.remove("content");
                            }
                        }
                        for call in tool_calls_to_responses(tool_calls)? {
                            output.push(call);
                        }
                    }
                } else if object.get("tool_calls").is_some() {
                    return Err(lossless());
                }
                if converted.get("content").is_some() || role != "assistant" {
                    output.push(Value::Object(converted));
                }
            }
            "tool" => {
                reject_object_keys(object, &["role", "tool_call_id", "content"])?;
                let call_id = object
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(lossless)?;
                let content = object
                    .get("content")
                    .and_then(Value::as_str)
                    .ok_or_else(lossless)?;
                output.push(json!({
                    "type": "function_call_output",
                    "call_id": call_id,
                    "output": content
                }));
            }
            _ => return Err(lossless()),
        }
    }
    Ok(Value::Array(output))
}

fn chat_content_to_responses(content: &Value, assistant: bool) -> Result<Value, ProtocolError> {
    match content {
        Value::Null => Ok(Value::Null),
        Value::String(_) => Ok(content.clone()),
        Value::Array(parts) => parts
            .iter()
            .map(|part| {
                let object = part.as_object().ok_or_else(lossless)?;
                reject_object_keys(object, &["type", "text"])?;
                if object.get("type").and_then(Value::as_str) != Some("text") {
                    return Err(lossless());
                }
                let text = object
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(lossless)?;
                Ok(json!({
                    "type": if assistant { "output_text" } else { "input_text" },
                    "text": text
                }))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        _ => Err(lossless()),
    }
}

fn tool_calls_to_responses(input: &Value) -> Result<Vec<Value>, ProtocolError> {
    input
        .as_array()
        .ok_or_else(lossless)?
        .iter()
        .map(|call| {
            let object = call.as_object().ok_or_else(lossless)?;
            reject_object_keys(object, &["id", "type", "function"])?;
            if object.get("type").and_then(Value::as_str) != Some("function") {
                return Err(lossless());
            }
            let id = object
                .get("id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(lossless)?;
            let function = object
                .get("function")
                .and_then(Value::as_object)
                .ok_or_else(lossless)?;
            reject_object_keys(function, &["name", "arguments"])?;
            let name = function
                .get("name")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(lossless)?;
            let arguments = function
                .get("arguments")
                .and_then(Value::as_str)
                .ok_or_else(lossless)?;
            Ok(json!({
                "type": "function_call",
                "id": id,
                "call_id": id,
                "name": name,
                "arguments": arguments
            }))
        })
        .collect()
}

fn responses_input_to_chat(input: &Value) -> Result<Value, ProtocolError> {
    match input {
        Value::String(text) => Ok(json!([{ "role": "user", "content": text }])),
        Value::Array(items) => items
            .iter()
            .map(responses_input_item_to_chat)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        _ => Err(lossless()),
    }
}

fn responses_input_item_to_chat(item: &Value) -> Result<Value, ProtocolError> {
    let object = item.as_object().ok_or_else(lossless)?;
    match object.get("type").and_then(Value::as_str) {
        Some("message") => {
            reject_object_keys(object, &["type", "role", "content"])?;
            let role = object
                .get("role")
                .and_then(Value::as_str)
                .ok_or_else(lossless)?;
            if !matches!(role, "user" | "system" | "developer" | "assistant") {
                return Err(lossless());
            }
            Ok(json!({
                "role": role,
                "content": responses_content_to_chat(object.get("content").ok_or_else(lossless)?)?
            }))
        }
        Some("function_call") => {
            reject_object_keys(object, &["type", "id", "call_id", "name", "arguments"])?;
            let id = object
                .get("id")
                .or_else(|| object.get("call_id"))
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(lossless)?;
            if object.get("id").is_some()
                && object.get("call_id").is_some()
                && object["id"] != object["call_id"]
            {
                return Err(lossless());
            }
            let name = object
                .get("name")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(lossless)?;
            let arguments = object
                .get("arguments")
                .and_then(Value::as_str)
                .ok_or_else(lossless)?;
            Ok(json!({
                "role": "assistant",
                "content": Value::Null,
                "tool_calls": [{"id": id, "type": "function", "function": {"name": name, "arguments": arguments}}]
            }))
        }
        Some("function_call_output") => {
            reject_object_keys(object, &["type", "call_id", "output"])?;
            let call_id = object
                .get("call_id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(lossless)?;
            let output = object
                .get("output")
                .and_then(Value::as_str)
                .ok_or_else(lossless)?;
            Ok(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": output
            }))
        }
        Some("reasoning") => {
            reject_object_keys(object, &["type", "summary"])?;
            let summary = object
                .get("summary")
                .and_then(Value::as_str)
                .ok_or_else(lossless)?;
            Ok(json!({
                "role": "assistant",
                "content": Value::Null,
                "reasoning_content": summary
            }))
        }
        _ => Err(lossless()),
    }
}

fn responses_content_to_chat(content: &Value) -> Result<Value, ProtocolError> {
    match content {
        Value::String(_) => Ok(content.clone()),
        Value::Array(parts) => parts
            .iter()
            .map(|part| {
                let object = part.as_object().ok_or_else(lossless)?;
                reject_object_keys(object, &["type", "text", "annotations"])?;
                if !matches!(
                    object.get("type").and_then(Value::as_str),
                    Some("input_text") | Some("output_text")
                ) {
                    return Err(lossless());
                }
                if object
                    .get("annotations")
                    .is_some_and(|annotations| !annotations.as_array().is_some_and(Vec::is_empty))
                {
                    return Err(lossless());
                }
                let text = object
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(lossless)?;
                Ok(json!({ "type": "text", "text": text }))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        _ => Err(lossless()),
    }
}

fn responses_to_chat_response(value: &Value, model: &str) -> Result<Value, ProtocolError> {
    if value.get("error").is_some() {
        return normalize_error(value);
    }
    let object = value.as_object().ok_or_else(lossless)?;
    reject_object_keys(
        object,
        &[
            "id",
            "object",
            "status",
            "model",
            "output",
            "usage",
            "incomplete_details",
        ],
    )?;
    let mut content = String::new();
    let mut tool_calls = Vec::new();
    let mut reasoning = String::new();
    for item in object
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(lossless)?
    {
        match item.get("type").and_then(Value::as_str) {
            Some("message") => {
                let item = item.as_object().ok_or_else(lossless)?;
                reject_object_keys(item, &["type", "role", "content"])?;
                if item
                    .get("role")
                    .and_then(Value::as_str)
                    .is_some_and(|role| role != "assistant")
                {
                    return Err(lossless());
                }
                for block in item
                    .get("content")
                    .and_then(Value::as_array)
                    .ok_or_else(lossless)?
                {
                    let block = block.as_object().ok_or_else(lossless)?;
                    match block.get("type").and_then(Value::as_str) {
                        Some("output_text") => {
                            reject_object_keys(block, &["type", "text", "annotations"])?;
                            if block.get("annotations").is_some_and(|annotations| {
                                !annotations.as_array().is_some_and(Vec::is_empty)
                            }) {
                                return Err(lossless());
                            }
                            content.push_str(
                                block
                                    .get("text")
                                    .and_then(Value::as_str)
                                    .ok_or_else(lossless)?,
                            );
                        }
                        Some("reasoning_text") => {
                            reject_object_keys(block, &["type", "text"])?;
                            reasoning.push_str(
                                block
                                    .get("text")
                                    .and_then(Value::as_str)
                                    .ok_or_else(lossless)?,
                            );
                        }
                        _ => return Err(lossless()),
                    }
                }
            }
            Some("function_call") => {
                let item = item.as_object().ok_or_else(lossless)?;
                reject_object_keys(
                    item,
                    &["type", "id", "call_id", "name", "arguments", "status"],
                )?;
                if item
                    .get("status")
                    .is_some_and(|status| status.as_str() != Some("completed"))
                {
                    return Err(lossless());
                }
                let id = item
                    .get("call_id")
                    .or_else(|| item.get("id"))
                    .and_then(Value::as_str)
                    .filter(|id| !id.is_empty())
                    .ok_or_else(lossless)?;
                if item.get("id").is_some()
                    && item.get("call_id").is_some()
                    && item["id"] != item["call_id"]
                {
                    return Err(lossless());
                }
                let name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|name| !name.is_empty())
                    .ok_or_else(lossless)?;
                let arguments = item
                    .get("arguments")
                    .and_then(Value::as_str)
                    .ok_or_else(lossless)?;
                tool_calls.push(json!({
                    "id": id,
                    "type": "function",
                    "function": {"name": name, "arguments": arguments}
                }));
            }
            Some("reasoning") => {
                let item = item.as_object().ok_or_else(lossless)?;
                reject_object_keys(item, &["type", "summary"])?;
                reasoning.push_str(
                    item.get("summary")
                        .and_then(Value::as_str)
                        .ok_or_else(lossless)?,
                );
            }
            _ => return Err(lossless()),
        }
    }
    let mut message = json!({ "role": "assistant", "content": content });
    let has_tool_calls = !tool_calls.is_empty();
    if has_tool_calls {
        message["tool_calls"] = Value::Array(tool_calls);
    }
    if !reasoning.is_empty() {
        message["reasoning_content"] = Value::String(reasoning);
    }
    let finish = response_finish_reason(object, has_tool_calls)?;
    let usage = responses_usage_to_chat(object.get("usage"))?;
    Ok(json!({
        "id": object.get("id").cloned().unwrap_or_else(|| Value::String(format!("chatcmpl_{}", uuid::Uuid::new_v4().simple()))),
        "object": "chat.completion", "model": model,
        "choices": [{ "index": 0, "message": message, "finish_reason": finish }],
        "usage": usage
    }))
}

fn chat_to_responses_response(value: &Value, model: &str) -> Result<Value, ProtocolError> {
    if value.get("error").is_some() {
        return normalize_error(value);
    }
    let object = value.as_object().ok_or_else(lossless)?;
    reject_object_keys(object, &["id", "object", "model", "choices", "usage"])?;
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| (choices.len() == 1).then(|| &choices[0]))
        .ok_or_else(lossless)?;
    let choice_object = choice.as_object().ok_or_else(lossless)?;
    reject_object_keys(choice_object, &["index", "message", "finish_reason"])?;
    let message = choice.get("message").ok_or_else(lossless)?;
    let message_object = message.as_object().ok_or_else(lossless)?;
    reject_object_keys(
        message_object,
        &["role", "content", "tool_calls", "reasoning_content"],
    )?;
    if message_object.get("role").and_then(Value::as_str) != Some("assistant") {
        return Err(lossless());
    }
    let mut output = Vec::new();
    let mut content = Vec::new();
    if let Some(content_value) = message.get("content") {
        if let Some(text) = content_value.as_str() {
            content.push(json!({ "type": "output_text", "text": text, "annotations": [] }));
        } else if !content_value.is_null() {
            return Err(lossless());
        }
    }
    if let Some(reasoning) = message.get("reasoning_content").and_then(Value::as_str) {
        content.push(json!({ "type": "reasoning_text", "text": reasoning }));
    } else if message.get("reasoning_content").is_some() {
        return Err(lossless());
    }
    if !content.is_empty() {
        output.push(json!({ "type": "message", "role": "assistant", "content": content }));
    }
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for call in calls {
            let call = call.as_object().ok_or_else(lossless)?;
            reject_object_keys(call, &["id", "type", "function"])?;
            if call.get("type").and_then(Value::as_str) != Some("function") {
                return Err(lossless());
            }
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .ok_or_else(lossless)?;
            let function = call.get("function").ok_or_else(lossless)?;
            let function = function.as_object().ok_or_else(lossless)?;
            reject_object_keys(function, &["name", "arguments"])?;
            output.push(json!({
                "type": "function_call", "id": id, "call_id": id,
                "name": function.get("name").and_then(Value::as_str).filter(|name| !name.is_empty()).ok_or_else(lossless)?,
                "arguments": function.get("arguments").and_then(Value::as_str).ok_or_else(lossless)?
            }));
        }
    } else if message.get("tool_calls").is_some() {
        return Err(lossless());
    }
    let finish = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .ok_or_else(lossless)?;
    if matches!(finish, "tool_calls" | "function_call")
        && output.iter().all(|item| item["type"] != "function_call")
    {
        return Err(lossless());
    }
    if !matches!(
        finish,
        "stop" | "length" | "content_filter" | "tool_calls" | "function_call"
    ) {
        return Err(lossless());
    }
    let (status, incomplete_details) = match finish {
        "length" => ("incomplete", Some(json!({"reason": "max_output_tokens"}))),
        "content_filter" => ("incomplete", Some(json!({"reason": "content_filter"}))),
        _ => ("completed", None),
    };
    let usage = chat_usage_to_responses(object.get("usage"))?;
    Ok(json!({
        "id": object.get("id").cloned().unwrap_or_else(|| Value::String(format!("resp_{}", uuid::Uuid::new_v4().simple()))),
        "object": "response", "model": model,
        "status": status,
        "incomplete_details": incomplete_details,
        "output": output, "usage": usage
    }))
}

fn response_finish_reason(
    value: &Map<String, Value>,
    has_tool_calls: bool,
) -> Result<&'static str, ProtocolError> {
    match value.get("status").and_then(Value::as_str) {
        None | Some("completed") => {
            if value
                .get("incomplete_details")
                .is_some_and(|details| !details.is_null())
            {
                return Err(lossless());
            }
            Ok(if has_tool_calls { "tool_calls" } else { "stop" })
        }
        Some("incomplete") if !has_tool_calls => match value
            .get("incomplete_details")
            .and_then(|details| details.get("reason"))
            .and_then(Value::as_str)
        {
            Some("max_output_tokens") => Ok("length"),
            Some("content_filter") => Ok("content_filter"),
            _ => Err(lossless()),
        },
        Some("incomplete") => Err(lossless()),
        Some("failed") | Some("cancelled") => Err(lossless()),
        Some(_) => Err(lossless()),
    }
}

fn convert_stream_event_with_state(
    state: &mut SseConversionState,
    value: &Value,
) -> Result<Vec<Value>, ProtocolError> {
    match (state.source, state.target) {
        (UpstreamProtocol::ChatCompletions, UpstreamProtocol::Responses) => {
            if value
                .get("choices")
                .is_some_and(|choices| !choices.is_array())
            {
                return Err(lossless());
            }
            let choices = value.get("choices").and_then(Value::as_array);
            if choices.is_some_and(|choices| choices.len() > 1) {
                return Err(lossless());
            }
            let choice = choices.and_then(|choices| choices.first());
            if let Some(id) = value.get("id").and_then(Value::as_str) {
                set_response_id(state, id);
            } else if value.get("id").is_some_and(|id| !id.is_null()) {
                return Err(lossless());
            }
            if choice
                .and_then(|choice| choice.get("index"))
                .is_some_and(|index| !index.is_u64())
            {
                return Err(lossless());
            }
            let mut events = Vec::new();
            if let Some(delta) = choice.and_then(|choice| choice.get("delta")) {
                if let Some(text) = delta.get("content").filter(|text| !text.is_null()) {
                    let text = text.as_str().ok_or_else(lossless)?;
                    events.push(json!({
                        "type": "response.output_text.delta",
                        "response_id": response_id(state),
                        "item_id": state.message_item_id,
                        "delta": text
                    }));
                }
                if let Some(reasoning) = delta
                    .get("reasoning_content")
                    .filter(|reasoning| !reasoning.is_null())
                {
                    let reasoning = reasoning.as_str().ok_or_else(lossless)?;
                    events.push(json!({
                        "type": "response.reasoning_summary_text.delta",
                        "response_id": response_id(state),
                        "item_id": state.message_item_id,
                        "delta": reasoning
                    }));
                }
                if let Some(calls) = delta.get("tool_calls").filter(|calls| !calls.is_null()) {
                    for call in calls.as_array().ok_or_else(lossless)? {
                        events.extend(chat_tool_events(state, call)?);
                    }
                }
            }
            if let Some(usage) = value.get("usage") {
                state.usage = Some(chat_usage_to_responses(Some(usage))?);
            }
            if let Some(finish_reason) = choice
                .and_then(|choice| choice.get("finish_reason"))
                .and_then(Value::as_str)
            {
                set_finish_reason(state, finish_reason)?;
            } else if choice
                .and_then(|choice| choice.get("finish_reason"))
                .is_some_and(|finish| !finish.is_null())
            {
                return Err(lossless());
            }
            Ok(events)
        }
        (UpstreamProtocol::Responses, UpstreamProtocol::ChatCompletions) => {
            match value.get("type").and_then(Value::as_str) {
                Some("response.created") => {
                    let response = value.get("response").ok_or_else(lossless)?;
                    let id = response
                        .get("id")
                        .and_then(Value::as_str)
                        .ok_or_else(lossless)?;
                    set_response_id(state, id);
                    Ok(Vec::new())
                }
                Some("response.output_item.added") => {
                    let item = value.get("item").ok_or_else(lossless)?;
                    if item.get("type").and_then(Value::as_str) != Some("function_call") {
                        return Ok(Vec::new());
                    }
                    let tool_index = register_response_tool(state, item)?;
                    let tool = state.tools[tool_index].clone();
                    Ok(vec![chat_chunk(
                        state,
                        json!({ "tool_calls": [{ "index": tool.index, "id": tool.id, "type": "function", "function": { "name": tool.name } }] }),
                        Value::Null,
                        None,
                    )])
                }
                Some("response.output_item.done") => {
                    if let Some(item) = value.get("item") {
                        if item.get("type").and_then(Value::as_str) == Some("function_call") {
                            register_response_tool(state, item)?;
                        }
                    }
                    Ok(Vec::new())
                }
                Some("response.output_text.delta") => {
                    let delta = value
                        .get("delta")
                        .and_then(Value::as_str)
                        .ok_or_else(lossless)?;
                    Ok(vec![chat_chunk(
                        state,
                        json!({ "content": delta }),
                        Value::Null,
                        None,
                    )])
                }
                Some("response.function_call_arguments.delta") => {
                    let item_id = value
                        .get("item_id")
                        .and_then(Value::as_str)
                        .ok_or_else(lossless)?;
                    let delta = value
                        .get("delta")
                        .and_then(Value::as_str)
                        .ok_or_else(lossless)?;
                    let tool_index = find_or_register_response_tool(state, item_id, value)?;
                    let (tool_id, tool_index_value, tool_name, output_item_emitted) = {
                        let tool = &mut state.tools[tool_index];
                        tool.arguments.push_str(delta);
                        let output_item_emitted = tool.output_item_emitted;
                        tool.output_item_emitted = true;
                        (
                            tool.id.clone(),
                            tool.index,
                            tool.name.clone(),
                            output_item_emitted,
                        )
                    };
                    let mut function = Map::new();
                    if !output_item_emitted {
                        function.insert("name".into(), Value::String(tool_name));
                    }
                    function.insert("arguments".into(), Value::String(delta.into()));
                    Ok(vec![chat_chunk(
                        state,
                        json!({ "tool_calls": [{ "index": tool_index_value, "id": tool_id, "type": "function", "function": Value::Object(function) }] }),
                        Value::Null,
                        None,
                    )])
                }
                Some("response.reasoning_summary_text.delta") => {
                    let delta = value
                        .get("delta")
                        .and_then(Value::as_str)
                        .ok_or_else(lossless)?;
                    Ok(vec![chat_chunk(
                        state,
                        json!({ "reasoning_content": delta }),
                        Value::Null,
                        None,
                    )])
                }
                Some("response.completed") | Some("response.incomplete") => {
                    let response = value.get("response").unwrap_or(value);
                    let mut events = Vec::new();
                    if let Some(id) = response.get("id").and_then(Value::as_str) {
                        set_response_id(state, id);
                    }
                    if let Some(output) = response.get("output").and_then(Value::as_array) {
                        for item in output {
                            if item.get("type").and_then(Value::as_str) != Some("function_call") {
                                continue;
                            }
                            let key = item
                                .get("id")
                                .or_else(|| item.get("call_id"))
                                .and_then(Value::as_str)
                                .ok_or_else(lossless)?;
                            let known = state.tools.iter().any(|tool| tool.key == key);
                            let tool_index = register_response_tool(state, item)?;
                            if !known {
                                let tool = state.tools[tool_index].clone();
                                events.push(chat_chunk(
                                    state,
                                    json!({ "tool_calls": [{ "index": tool.index, "id": tool.id, "type": "function", "function": { "name": tool.name, "arguments": tool.arguments } }] }),
                                    Value::Null,
                                    None,
                                ));
                            }
                        }
                    }
                    if let Some(usage) = response.get("usage") {
                        state.usage = Some(responses_usage_to_chat(Some(usage))?);
                    }
                    if value.get("type").and_then(Value::as_str) == Some("response.incomplete") {
                        if !state.tools.is_empty() {
                            return Err(lossless());
                        }
                        let reason = response
                            .get("incomplete_details")
                            .and_then(|details| details.get("reason"))
                            .and_then(Value::as_str)
                            .ok_or_else(lossless)?;
                        state.finish_reason = Some(map_response_incomplete_reason(reason)?.into());
                    } else {
                        state.finish_reason = Some(
                            response_finish_reason(
                                response.as_object().ok_or_else(lossless)?,
                                !state.tools.is_empty(),
                            )?
                            .into(),
                        );
                    }
                    events.extend(finish_chat_completion(state)?);
                    Ok(events)
                }
                Some("error") => Ok(vec![normalize_error(value)?]),
                Some("response.in_progress")
                | Some("response.output_text.done")
                | Some("response.content_part.added")
                | Some("response.content_part.done") => Ok(Vec::new()),
                Some(_) => Err(lossless()),
                None => Err(lossless()),
            }
        }
        _ => unreachable!(),
    }
}

fn chat_chunk(
    state: &SseConversionState,
    delta: Value,
    finish_reason: Value,
    usage: Option<&Value>,
) -> Value {
    json!({
        "id": response_id(state), "object": "chat.completion.chunk", "model": state.model,
        "choices": [{ "index": 0, "delta": delta, "finish_reason": finish_reason }],
        "usage": usage.cloned().or_else(|| state.usage.clone()).unwrap_or(Value::Null)
    })
}

fn finish_sse_state(state: &mut SseConversionState) -> Result<Vec<Value>, ProtocolError> {
    if state.source == state.target || state.completion_emitted {
        return Ok(Vec::new());
    }
    match (state.source, state.target) {
        (UpstreamProtocol::ChatCompletions, UpstreamProtocol::Responses) => {
            if state.finish_reason.is_none() {
                state.finish_reason = Some("stop".into());
            }
            let finish_reason = state.finish_reason.as_deref().ok_or_else(lossless)?;
            let (status, incomplete_details) = match finish_reason {
                "length" => ("incomplete", Some(json!({"reason": "max_output_tokens"}))),
                "content_filter" => ("incomplete", Some(json!({"reason": "content_filter"}))),
                "stop" | "tool_calls" => ("completed", None),
                _ => return Err(lossless()),
            };
            let output = state
                .tools
                .iter()
                .map(|tool| {
                    json!({
                        "type": "function_call",
                        "id": tool.id,
                        "call_id": tool.id,
                        "name": tool.name,
                        "arguments": tool.arguments
                    })
                })
                .collect::<Vec<_>>();
            state.completion_emitted = true;
            Ok(vec![json!({
                "type": if status == "completed" { "response.completed" } else { "response.incomplete" },
                "response": {
                    "id": response_id(state),
                    "object": "response",
                    "status": status,
                    "model": state.model,
                    "output": output,
                    "incomplete_details": incomplete_details,
                    "usage": state.usage.clone().unwrap_or(Value::Null)
                }
            })])
        }
        (UpstreamProtocol::Responses, UpstreamProtocol::ChatCompletions) => {
            state.finish_reason.get_or_insert_with(|| "stop".into());
            let result = chat_chunk(
                state,
                json!({}),
                Value::String(state.finish_reason.clone().ok_or_else(lossless)?.into()),
                None,
            );
            state.completion_emitted = true;
            Ok(vec![result])
        }
        _ => unreachable!(),
    }
}

fn finish_chat_completion(state: &mut SseConversionState) -> Result<Vec<Value>, ProtocolError> {
    if state.completion_emitted {
        return Ok(Vec::new());
    }
    let finish = state.finish_reason.clone().ok_or_else(lossless)?;
    state.completion_emitted = true;
    Ok(vec![chat_chunk(
        state,
        json!({}),
        Value::String(finish),
        None,
    )])
}

fn set_response_id(state: &mut SseConversionState, id: &str) {
    if state.response_id.is_none() && !id.is_empty() {
        state.response_id = Some(id.to_owned());
    }
}

fn response_id(state: &SseConversionState) -> String {
    let id = state
        .response_id
        .clone()
        .unwrap_or_else(|| format!("chatcmpl_{}", state.request_id.trim_start_matches("req_")));
    if state.target == UpstreamProtocol::ChatCompletions && !id.starts_with("chatcmpl-") {
        format!("chatcmpl-{id}")
    } else {
        id
    }
}

fn set_finish_reason(
    state: &mut SseConversionState,
    finish_reason: &str,
) -> Result<(), ProtocolError> {
    if !matches!(
        finish_reason,
        "stop" | "length" | "content_filter" | "tool_calls" | "function_call"
    ) {
        return Err(lossless());
    }
    state.finish_reason = Some(
        if finish_reason == "function_call" {
            "tool_calls"
        } else {
            finish_reason
        }
        .into(),
    );
    Ok(())
}

fn map_response_incomplete_reason(reason: &str) -> Result<&'static str, ProtocolError> {
    match reason {
        "max_output_tokens" => Ok("length"),
        "content_filter" => Ok("content_filter"),
        _ => Err(lossless()),
    }
}

fn chat_tool_events(
    state: &mut SseConversionState,
    call: &Value,
) -> Result<Vec<Value>, ProtocolError> {
    let object = call.as_object().ok_or_else(lossless)?;
    reject_object_keys(object, &["index", "id", "type", "function"])?;
    if object.get("type").and_then(Value::as_str) != Some("function") {
        return Err(lossless());
    }
    let index = object
        .get("index")
        .and_then(Value::as_u64)
        .ok_or_else(lossless)? as usize;
    let id = match object.get("id") {
        Some(value) => value.as_str().map(str::to_owned).ok_or_else(lossless)?,
        None => format!("call_{index}"),
    };
    let function = object
        .get("function")
        .and_then(Value::as_object)
        .ok_or_else(lossless)?;
    reject_object_keys(function, &["name", "arguments"])?;
    let name = match function.get("name") {
        Some(value) => value.as_str().ok_or_else(lossless)?,
        None => "",
    };
    let arguments = match function.get("arguments") {
        Some(value) => value.as_str().ok_or_else(lossless)?,
        None => "",
    };
    let key = format!("index:{index}");
    let tool_index = if let Some(tool_index) = state.tools.iter().position(|tool| tool.key == key) {
        let tool = &mut state.tools[tool_index];
        if tool.id != id {
            return Err(lossless());
        }
        if !name.is_empty() && !tool.name.is_empty() && tool.name != name {
            return Err(lossless());
        }
        if tool.name.is_empty() && !name.is_empty() {
            tool.name = name.to_owned();
        }
        tool.arguments.push_str(arguments);
        tool_index
    } else {
        state.tools.push(StreamToolState {
            key,
            index,
            id,
            name: name.to_owned(),
            arguments: arguments.to_owned(),
            output_item_emitted: false,
        });
        state.tools.len() - 1
    };
    if state.tools[tool_index].name.is_empty() && !arguments.is_empty() {
        return Err(lossless());
    }
    let mut events = Vec::new();
    if !state.tools[tool_index].output_item_emitted {
        if state.tools[tool_index].name.is_empty() {
            return Ok(Vec::new());
        }
        let response_id = response_id(state);
        let tool = state.tools[tool_index].clone();
        events.push(json!({
            "type": "response.output_item.added",
            "response_id": response_id,
            "item": {"type": "function_call", "id": tool.id, "call_id": tool.id, "name": tool.name}
        }));
        state.tools[tool_index].output_item_emitted = true;
    }
    let response_id = response_id(state);
    let tool = state.tools[tool_index].clone();
    events.push(json!({
        "type": "response.function_call_arguments.delta",
        "response_id": response_id,
        "item_id": tool.id,
        "delta": arguments
    }));
    Ok(events)
}

fn register_response_tool(
    state: &mut SseConversionState,
    item: &Value,
) -> Result<usize, ProtocolError> {
    let object = item.as_object().ok_or_else(lossless)?;
    reject_object_keys(object, &["type", "id", "call_id", "name", "arguments"])?;
    let key = object
        .get("id")
        .or_else(|| object.get("call_id"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(lossless)?;
    let id = object
        .get("call_id")
        .or_else(|| object.get("id"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(lossless)?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(lossless)?;
    let key = key.to_owned();
    if let Some(existing_index) = state.tools.iter().position(|tool| tool.key == key) {
        let existing = &state.tools[existing_index];
        if existing.name != name {
            return Err(lossless());
        }
        return Ok(existing_index);
    }
    let index = state.tools.len();
    state.tools.push(StreamToolState {
        key,
        index,
        id: id.to_owned(),
        name: name.to_owned(),
        arguments: object
            .get("arguments")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        output_item_emitted: true,
    });
    Ok(index)
}

fn find_or_register_response_tool(
    state: &mut SseConversionState,
    item_id: &str,
    event: &Value,
) -> Result<usize, ProtocolError> {
    if let Some(tool_index) = state.tools.iter().position(|tool| tool.key == item_id) {
        return Ok(tool_index);
    }
    let name = event
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(lossless)?;
    let index = state.tools.len();
    state.tools.push(StreamToolState {
        key: item_id.to_owned(),
        index,
        id: event
            .get("call_id")
            .and_then(Value::as_str)
            .unwrap_or(item_id)
            .to_owned(),
        name: name.to_owned(),
        arguments: String::new(),
        output_item_emitted: false,
    });
    Ok(index)
}

fn normalize_error(value: &Value) -> Result<Value, ProtocolError> {
    let error = value.get("error").unwrap_or(value);
    let object = error.as_object().ok_or_else(lossless)?;
    reject_object_keys(object, &["message", "type", "param", "code"])?;
    let mut envelope = error_envelope(
        object
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("upstream_unavailable"),
        object
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Upstream request failed"),
    );
    if let Some(error_output) = envelope.get_mut("error").and_then(Value::as_object_mut) {
        if let Some(value) = object.get("type") {
            error_output.insert("type".into(), value.clone());
        }
        if let Some(value) = object.get("param") {
            error_output.insert("param".into(), value.clone());
        }
    }
    Ok(envelope)
}

fn responses_usage_to_chat(usage: Option<&Value>) -> Result<Value, ProtocolError> {
    let Some(usage) = usage else {
        return Ok(Value::Null);
    };
    let usage = usage.as_object().ok_or_else(lossless)?;
    reject_object_keys(
        usage,
        &[
            "input_tokens",
            "output_tokens",
            "total_tokens",
            "input_tokens_details",
            "output_tokens_details",
        ],
    )?;
    let input = usage.get("input_tokens").cloned().unwrap_or(Value::Null);
    let output = usage.get("output_tokens").cloned().unwrap_or(Value::Null);
    let mut result = json!({ "prompt_tokens": input, "completion_tokens": output, "total_tokens": usage.get("total_tokens").cloned().unwrap_or_else(|| sum_numbers(&input, &output)) });
    if let Some(details) = usage.get("input_tokens_details") {
        let details = details.as_object().ok_or_else(lossless)?;
        reject_object_keys(details, &["cached_tokens"])?;
        result["prompt_tokens_details"] = json!({
            "cached_tokens": details.get("cached_tokens").cloned().unwrap_or(Value::Null)
        });
    }
    if let Some(details) = usage.get("output_tokens_details") {
        let details = details.as_object().ok_or_else(lossless)?;
        reject_object_keys(details, &["reasoning_tokens"])?;
        result["completion_tokens_details"] = json!({
            "reasoning_tokens": details.get("reasoning_tokens").cloned().unwrap_or(Value::Null)
        });
    }
    Ok(result)
}

fn chat_usage_to_responses(usage: Option<&Value>) -> Result<Value, ProtocolError> {
    let Some(usage) = usage else {
        return Ok(Value::Null);
    };
    let usage = usage.as_object().ok_or_else(lossless)?;
    reject_object_keys(
        usage,
        &[
            "prompt_tokens",
            "completion_tokens",
            "total_tokens",
            "prompt_tokens_details",
            "completion_tokens_details",
        ],
    )?;
    let input = usage.get("prompt_tokens").cloned().unwrap_or(Value::Null);
    let output = usage
        .get("completion_tokens")
        .cloned()
        .unwrap_or(Value::Null);
    let mut result = json!({ "input_tokens": input, "output_tokens": output, "total_tokens": usage.get("total_tokens").cloned().unwrap_or_else(|| sum_numbers(&input, &output)) });
    if let Some(details) = usage.get("prompt_tokens_details") {
        let details = details.as_object().ok_or_else(lossless)?;
        reject_object_keys(details, &["cached_tokens"])?;
        result["input_tokens_details"] = json!({
            "cached_tokens": details.get("cached_tokens").cloned().unwrap_or(Value::Null)
        });
    }
    if let Some(details) = usage.get("completion_tokens_details") {
        let details = details.as_object().ok_or_else(lossless)?;
        reject_object_keys(details, &["reasoning_tokens"])?;
        result["output_tokens_details"] = json!({
            "reasoning_tokens": details.get("reasoning_tokens").cloned().unwrap_or(Value::Null)
        });
    }
    Ok(result)
}

fn sum_numbers(left: &Value, right: &Value) -> Value {
    match (left.as_u64(), right.as_u64()) {
        (Some(left), Some(right)) => Value::from(left + right),
        _ => Value::Null,
    }
}

fn copy(input: &Value, output: &mut Map<String, Value>, source: &str, target: &str) {
    if let Some(value) = input.get(source) {
        output.insert(target.into(), value.clone());
    }
}

fn chat_tools_to_responses(tools: &Value) -> Result<Value, ProtocolError> {
    let tools = tools.as_array().ok_or_else(lossless)?;
    tools
        .iter()
        .map(|tool| {
            let tool = tool.as_object().ok_or_else(lossless)?;
            reject_object_keys(tool, &["type", "function"])?;
            if tool.get("type").and_then(Value::as_str) != Some("function") {
                return Err(lossless());
            }
            let function = tool
                .get("function")
                .and_then(Value::as_object)
                .ok_or_else(lossless)?;
            reject_object_keys(function, &["name", "description", "parameters", "strict"])?;
            let name = function
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .ok_or_else(lossless)?;
            Ok(json!({
                "type": "function",
                "name": name,
                "description": function.get("description").cloned().unwrap_or(Value::Null),
                "parameters": function.get("parameters").cloned().unwrap_or_else(|| json!({ "type": "object", "properties": {} })),
                "strict": function.get("strict").cloned().unwrap_or(Value::Null)
            }))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

fn responses_tools_to_chat(tools: &Value) -> Result<Value, ProtocolError> {
    let tools = tools.as_array().ok_or_else(lossless)?;
    tools
        .iter()
        .map(|tool| {
            let tool = tool.as_object().ok_or_else(lossless)?;
            reject_object_keys(tool, &["type", "name", "description", "parameters", "strict"])?;
            if tool.get("type").and_then(Value::as_str) != Some("function") {
                return Err(lossless());
            }
            let name = tool
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .ok_or_else(lossless)?;
            Ok(json!({
                "type": "function",
                "function": {
                    "name": name,
                    "description": tool.get("description").cloned().unwrap_or(Value::Null),
                    "parameters": tool.get("parameters").cloned().unwrap_or_else(|| json!({ "type": "object", "properties": {} })),
                    "strict": tool.get("strict").cloned().unwrap_or(Value::Null)
                }
            }))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

fn chat_tool_choice_to_responses(choice: &Value) -> Result<Value, ProtocolError> {
    if choice.is_string() {
        if matches!(choice.as_str(), Some("none" | "auto" | "required")) {
            return Ok(choice.clone());
        }
        return Err(lossless());
    }
    let object = choice.as_object().ok_or_else(lossless)?;
    reject_object_keys(object, &["type", "function"])?;
    if object.get("type").and_then(Value::as_str) != Some("function") {
        return Err(lossless());
    }
    let name = object
        .get("function")
        .and_then(|function| function.get("name"))
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(lossless)?;
    Ok(json!({ "type": "function", "name": name }))
}

fn responses_tool_choice_to_chat(choice: &Value) -> Result<Value, ProtocolError> {
    if choice.is_string() {
        if matches!(choice.as_str(), Some("none" | "auto" | "required")) {
            return Ok(choice.clone());
        }
        return Err(lossless());
    }
    let object = choice.as_object().ok_or_else(lossless)?;
    reject_object_keys(object, &["type", "name"])?;
    if object.get("type").and_then(Value::as_str) != Some("function") {
        return Err(lossless());
    }
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(lossless)?;
    Ok(json!({ "type": "function", "function": { "name": name } }))
}

fn reject_object_keys(object: &Map<String, Value>, allowed: &[&str]) -> Result<(), ProtocolError> {
    if object
        .keys()
        .any(|key| !allowed.iter().any(|allowed| *allowed == key))
    {
        Err(lossless())
    } else {
        Ok(())
    }
}

fn reject_unknown(
    object: &Map<String, Value>,
    allowed: &HashSet<&str>,
) -> Result<(), ProtocolError> {
    if object.keys().any(|key| !allowed.contains(key.as_str())) {
        Err(lossless())
    } else {
        Ok(())
    }
}

fn responses_request_fields() -> HashSet<&'static str> {
    [
        "model",
        "input",
        "stream",
        "temperature",
        "max_output_tokens",
        "tools",
        "tool_choice",
        "parallel_tool_calls",
        "reasoning",
        "metadata",
        "store",
        "user",
    ]
    .into_iter()
    .collect()
}

fn chat_request_fields() -> HashSet<&'static str> {
    [
        "model",
        "messages",
        "stream",
        "stream_options",
        "temperature",
        "max_tokens",
        "tools",
        "tool_choice",
        "parallel_tool_calls",
        "reasoning_effort",
        "metadata",
        "store",
        "user",
    ]
    .into_iter()
    .collect()
}

fn lossless() -> ProtocolError {
    ProtocolError {
        code: "lossless_conversion_unsupported",
        message: "The request cannot be converted without losing information",
    }
}

fn invalid_request() -> ProtocolError {
    ProtocolError {
        code: "invalid_request",
        message: "The request is invalid",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bidirectional_json_preserves_tools_reasoning_usage_and_finish_reason() {
        let chat = json!({ "model": "public", "messages": [{"role":"user","content":"hi"}], "tools": [{"type":"function","function":{"name":"lookup","parameters":{"type":"object"}}}], "reasoning_effort": "high", "stream": false });
        let responses = convert_request(
            UpstreamProtocol::ChatCompletions,
            UpstreamProtocol::Responses,
            &chat,
            "vendor",
        )
        .unwrap();
        assert_eq!(responses["reasoning"]["effort"], "high");
        assert_eq!(responses["tools"][0]["name"], "lookup");
        let upstream = json!({ "id":"resp_1", "status":"completed", "output":[{"type":"message","content":[{"type":"output_text","text":"ok"},{"type":"reasoning_text","text":"because"}]},{"type":"function_call","call_id":"call_1","name":"lookup","arguments":"{}"}], "usage":{"input_tokens":3,"output_tokens":4,"total_tokens":7} });
        let converted = convert_response(
            UpstreamProtocol::Responses,
            UpstreamProtocol::ChatCompletions,
            &upstream,
            "public",
        )
        .unwrap();
        assert_eq!(
            converted["choices"][0]["message"]["tool_calls"][0]["function"]["name"],
            "lookup"
        );
        assert_eq!(
            converted["choices"][0]["message"]["reasoning_content"],
            "because"
        );
        assert_eq!(converted["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(converted["usage"]["total_tokens"], 7);
    }

    #[test]
    fn bidirectional_sse_preserves_text_tools_reasoning_usage_and_completion() {
        let responses = concat!(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n\n",
            "data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"think\"}\n\n",
            "data: {\"type\":\"response.output_item.added\",\"item\":{\"id\":\"item-1\",\"call_id\":\"call_1\",\"type\":\"function_call\",\"name\":\"lookup\"}}\n\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"item-1\",\"delta\":\"{}\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n",
            "data: [DONE]\n\n"
        );
        let chat = String::from_utf8(
            convert_sse(
                UpstreamProtocol::Responses,
                UpstreamProtocol::ChatCompletions,
                responses.as_bytes(),
                "public",
            )
            .unwrap(),
        )
        .unwrap();
        assert!(chat.contains("hello"));
        assert!(chat.contains("reasoning_content"));
        assert!(chat.contains("tool_calls"));
        assert!(chat.contains("total_tokens"));
        assert!(chat.contains("[DONE]"));
    }

    #[test]
    fn unsupported_fields_fail_before_upstream_conversion() {
        let value =
            json!({ "model":"public", "messages":[], "response_format":{"type":"json_schema"} });
        assert_eq!(
            convert_request(
                UpstreamProtocol::ChatCompletions,
                UpstreamProtocol::Responses,
                &value,
                "vendor"
            )
            .unwrap_err()
            .code,
            "lossless_conversion_unsupported"
        );
    }

    #[test]
    fn cross_protocol_request_preserves_common_metadata_and_rejects_stream_options() {
        let chat = json!({
            "model": "public",
            "messages": [{"role": "user", "content": "hi"}],
            "metadata": {"trace": "one"},
            "store": true,
            "user": "user-1"
        });
        let responses = convert_request(
            UpstreamProtocol::ChatCompletions,
            UpstreamProtocol::Responses,
            &chat,
            "vendor",
        )
        .unwrap();
        assert_eq!(responses["metadata"], chat["metadata"]);
        assert_eq!(responses["store"], chat["store"]);
        assert_eq!(responses["user"], chat["user"]);

        let responses = json!({
            "model": "public",
            "input": "hi",
            "metadata": {"trace": "two"},
            "store": false,
            "user": "user-2"
        });
        let chat = convert_request(
            UpstreamProtocol::Responses,
            UpstreamProtocol::ChatCompletions,
            &responses,
            "vendor",
        )
        .unwrap();
        assert_eq!(chat["metadata"], responses["metadata"]);
        assert_eq!(chat["store"], responses["store"]);
        assert_eq!(chat["user"], responses["user"]);

        let unsupported = json!({
            "model": "public",
            "messages": [{"role": "user", "content": "hi"}],
            "stream_options": {"include_usage": true}
        });
        assert_eq!(
            convert_request(
                UpstreamProtocol::ChatCompletions,
                UpstreamProtocol::Responses,
                &unsupported,
                "vendor"
            )
            .unwrap_err()
            .code,
            "lossless_conversion_unsupported"
        );
    }

    #[test]
    fn cross_protocol_request_maps_nested_tool_call_and_tool_result_items() {
        let chat = json!({
            "model": "public",
            "messages": [
                {"role": "assistant", "content": null, "tool_calls": [{"id": "call-1", "type": "function", "function": {"name": "lookup", "arguments": "{\"q\":\"x\"}"}}]},
                {"role": "tool", "tool_call_id": "call-1", "content": "result"}
            ]
        });
        let responses = convert_request(
            UpstreamProtocol::ChatCompletions,
            UpstreamProtocol::Responses,
            &chat,
            "vendor",
        )
        .unwrap();
        assert_eq!(responses["input"][0]["type"], "function_call");
        assert_eq!(responses["input"][0]["call_id"], "call-1");
        assert_eq!(responses["input"][0]["name"], "lookup");
        assert_eq!(responses["input"][1]["type"], "function_call_output");
        assert_eq!(responses["input"][1]["output"], "result");

        let chat = convert_request(
            UpstreamProtocol::Responses,
            UpstreamProtocol::ChatCompletions,
            &responses,
            "vendor",
        )
        .unwrap();
        assert_eq!(chat["messages"][0]["role"], "assistant");
        assert_eq!(chat["messages"][0]["tool_calls"][0]["id"], "call-1");
        assert_eq!(
            chat["messages"][0]["tool_calls"][0]["function"]["name"],
            "lookup"
        );
        assert_eq!(chat["messages"][1]["role"], "tool");
        assert_eq!(chat["messages"][1]["tool_call_id"], "call-1");
    }

    #[test]
    fn json_conversion_preserves_content_filter_finish_reason_and_usage_details() {
        let chat = json!({
            "id": "chatcmpl-1",
            "model": "public",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "blocked"},
                "finish_reason": "content_filter"
            }],
            "usage": {
                "prompt_tokens": 3,
                "completion_tokens": 4,
                "total_tokens": 7,
                "prompt_tokens_details": {"cached_tokens": 2},
                "completion_tokens_details": {"reasoning_tokens": 1}
            }
        });
        let responses = convert_response(
            UpstreamProtocol::ChatCompletions,
            UpstreamProtocol::Responses,
            &chat,
            "public",
        )
        .unwrap();
        assert_eq!(responses["status"], "incomplete");
        assert_eq!(responses["incomplete_details"]["reason"], "content_filter");
        assert_eq!(
            responses["usage"]["input_tokens_details"]["cached_tokens"],
            2
        );
        assert_eq!(
            responses["usage"]["output_tokens_details"]["reasoning_tokens"],
            1
        );

        let chat = convert_response(
            UpstreamProtocol::Responses,
            UpstreamProtocol::ChatCompletions,
            &json!({
                "id": "resp-1",
                "status": "incomplete",
                "incomplete_details": {"reason": "content_filter"},
                "output": [{"type": "message", "content": [{"type": "output_text", "text": "blocked"}]}],
                "usage": {"input_tokens": 3, "output_tokens": 4, "total_tokens": 7, "input_tokens_details": {"cached_tokens": 2}, "output_tokens_details": {"reasoning_tokens": 1}}
            }),
            "public",
        )
        .unwrap();
        assert_eq!(chat["choices"][0]["finish_reason"], "content_filter");
        assert_eq!(chat["usage"]["prompt_tokens_details"]["cached_tokens"], 2);
        assert_eq!(
            chat["usage"]["completion_tokens_details"]["reasoning_tokens"],
            1
        );
    }

    #[test]
    fn error_conversion_preserves_the_common_error_fields_or_rejects_extra_fields() {
        let converted = convert_response(
            UpstreamProtocol::Responses,
            UpstreamProtocol::ChatCompletions,
            &json!({
                "error": {
                    "message": "bad",
                    "type": "invalid_request_error",
                    "param": "input",
                    "code": "invalid_request"
                }
            }),
            "public",
        )
        .unwrap();
        assert_eq!(converted["error"]["type"], "invalid_request_error");
        assert_eq!(converted["error"]["param"], "input");
        assert_eq!(converted["error"]["code"], "invalid_request");
        assert_eq!(
            convert_response(
                UpstreamProtocol::Responses,
                UpstreamProtocol::ChatCompletions,
                &json!({"error":{"message":"bad","extra":"cannot-map"}}),
                "public"
            )
            .unwrap_err()
            .code,
            "lossless_conversion_unsupported"
        );
    }

    #[test]
    fn sse_conversion_keeps_one_id_and_tool_name_across_events() {
        let responses = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp-stream-1\"}}\n\n",
            "data: {\"type\":\"response.output_item.added\",\"item\":{\"id\":\"item-1\",\"call_id\":\"call-1\",\"type\":\"function_call\",\"name\":\"lookup\"}}\n\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"item-1\",\"delta\":\"{}\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp-stream-1\",\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n",
            "data: [DONE]\n\n"
        );
        let chat = String::from_utf8(
            convert_sse(
                UpstreamProtocol::Responses,
                UpstreamProtocol::ChatCompletions,
                responses.as_bytes(),
                "public",
            )
            .unwrap(),
        )
        .unwrap();
        let ids: Vec<String> = chat
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|value| value["id"].as_str().map(str::to_owned))
            .collect();
        assert!(!ids.is_empty());
        assert!(ids.iter().all(|id| id == "chatcmpl-resp-stream-1"));
        assert!(chat.contains("\"name\":\"lookup\""));
        assert!(chat.contains("\"finish_reason\":\"tool_calls\""));
        assert!(chat.contains("\"total_tokens\":3"));
    }

    #[test]
    fn chat_to_responses_sse_keeps_id_tools_finish_reason_and_usage() {
        let chat = concat!(
            "data: {\"id\":\"chatcmpl-stream-1\",\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl-stream-1\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call-1\",\"type\":\"function\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl-stream-1\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call-1\",\"type\":\"function\",\"function\":{\"arguments\":\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: {\"id\":\"chatcmpl-stream-1\",\"choices\":[],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":2,\"total_tokens\":3}}\n\n",
            "data: [DONE]\n\n"
        );
        let responses = String::from_utf8(
            convert_sse(
                UpstreamProtocol::ChatCompletions,
                UpstreamProtocol::Responses,
                chat.as_bytes(),
                "public",
            )
            .unwrap(),
        )
        .unwrap();
        assert!(responses.contains("response.output_text.delta"));
        assert!(responses.contains("response.output_item.added"));
        assert!(responses.contains("\"name\":\"lookup\""));
        assert!(responses.contains("response.function_call_arguments.delta"));
        assert!(responses.contains("\"status\":\"completed\""));
        assert!(responses.contains("\"total_tokens\":3"));
        assert!(responses.contains("\"id\":\"chatcmpl-stream-1\""));
    }
}
