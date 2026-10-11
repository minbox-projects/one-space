//! Native Anthropic Messages protocol helpers.

use serde_json::{json, Map, Value};

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

pub(in crate::ai_gateway) fn is_conversion_path(path: &str) -> bool {
    matches!(
        path.split('?').next().unwrap_or(path),
        "/v1/chat/completions" | "/v1/responses"
    )
}

fn field_error(field: &str, message: impl Into<String>) -> String {
    format!("field '{field}': {}", message.into())
}

fn required_object<'a>(body: &'a Value) -> Result<&'a Map<String, Value>, String> {
    body.as_object()
        .ok_or_else(|| "request body must be a JSON object".to_string())
}

fn output_cap(body: &Map<String, Value>, fields: &[&str]) -> Result<u64, String> {
    for field in fields {
        if let Some(value) = body.get(*field) {
            return value
                .as_u64()
                .ok_or_else(|| field_error(field, "must be a non-negative integer"));
        }
    }
    Ok(4096)
}

fn parse_image_url(url: &str) -> Result<Value, String> {
    if let Some(data) = url.strip_prefix("data:") {
        let Some((header, encoded)) = data.split_once(",") else {
            return Err("image_url: invalid data URI".to_string());
        };
        let Some(media_type) = header.strip_suffix(";base64") else {
            return Err("image_url: data URI must be base64 encoded".to_string());
        };
        if media_type.trim().is_empty() || encoded.is_empty() {
            return Err("image_url: data URI is incomplete".to_string());
        }
        Ok(json!({
            "type": "image",
            "source": {"type": "base64", "media_type": media_type, "data": encoded}
        }))
    } else if url.trim().is_empty() {
        Err("image_url: URL must not be empty".to_string())
    } else {
        Ok(json!({
            "type": "image",
            "source": {"type": "url", "url": url}
        }))
    }
}

fn chat_content_blocks(content: &Value) -> Result<Vec<Value>, String> {
    if let Some(text) = content.as_str() {
        return Ok(vec![json!({"type": "text", "text": text})]);
    }
    let Some(items) = content.as_array() else {
        return Err("message.content: expected text or content blocks".to_string());
    };
    items
        .iter()
        .map(|item| {
            let object = item
                .as_object()
                .ok_or_else(|| "message.content: invalid block".to_string())?;
            match object.get("type").and_then(Value::as_str) {
                Some("text") => Ok(json!({
                    "type": "text",
                    "text": object.get("text").and_then(Value::as_str).ok_or_else(|| {
                        "message.content.text: expected a string".to_string()
                    })?
                })),
                Some("image_url") => {
                    let url = object
                        .get("image_url")
                        .and_then(|value| value.get("url"))
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            "message.content.image_url.url: expected a string".to_string()
                        })?;
                    parse_image_url(url)
                }
                Some(other) => Err(format!("message.content.{other}: unsupported block type")),
                None => Err("message.content.type: field is required".to_string()),
            }
        })
        .collect()
}

fn response_content_blocks(content: &Value) -> Result<Vec<Value>, String> {
    if let Some(text) = content.as_str() {
        return Ok(vec![json!({"type": "text", "text": text})]);
    }
    let Some(items) = content.as_array() else {
        return Err("input.content: expected text or content blocks".to_string());
    };
    items
        .iter()
        .map(|item| {
            let object = item
                .as_object()
                .ok_or_else(|| "input.content: invalid block".to_string())?;
            match object.get("type").and_then(Value::as_str) {
                Some("input_text") | Some("output_text") | Some("text") => Ok(json!({
                    "type": "text",
                    "text": object.get("text").and_then(Value::as_str).ok_or_else(|| {
                        "input.content.text: expected a string".to_string()
                    })?
                })),
                Some("input_image") => {
                    let url = object
                        .get("image_url")
                        .and_then(|value| {
                            value
                                .as_str()
                                .or_else(|| value.get("url").and_then(Value::as_str))
                        })
                        .ok_or_else(|| "input.content.image_url: expected a URL".to_string())?;
                    parse_image_url(url)
                }
                Some(other) => Err(format!("input.content.{other}: unsupported block type")),
                None => Err("input.content.type: field is required".to_string()),
            }
        })
        .collect()
}

fn tool_choice(value: Option<&Value>) -> Result<Option<Value>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if let Some(choice) = value.as_str() {
        return match choice {
            "auto" => Ok(Some(json!({"type": "auto"}))),
            "required" => Ok(Some(json!({"type": "any"}))),
            "none" => Ok(None),
            _ => Err(field_error("tool_choice", "unsupported choice")),
        };
    }
    let object = value
        .as_object()
        .ok_or_else(|| field_error("tool_choice", "must be a string or object"))?;
    match object.get("type").and_then(Value::as_str) {
        Some("function") => Ok(Some(json!({
            "type": "tool",
            "name": object
                .get("function")
                .and_then(|function| function.get("name"))
                .and_then(Value::as_str)
                .ok_or_else(|| field_error("tool_choice.function.name", "is required"))?
        }))),
        Some("auto") => Ok(Some(json!({"type": "auto"}))),
        Some("required") => Ok(Some(json!({"type": "any"}))),
        Some("none") => Ok(None),
        _ => Err(field_error("tool_choice", "unsupported choice")),
    }
}

fn convert_tools(value: Option<&Value>) -> Result<Option<Vec<Value>>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let tools = value
        .as_array()
        .ok_or_else(|| field_error("tools", "must be an array"))?;
    tools
        .iter()
        .map(|tool| {
            let object = tool
                .as_object()
                .ok_or_else(|| field_error("tools", "contains an invalid tool"))?;
            let function = if object.get("type").and_then(Value::as_str) == Some("function") {
                object
                    .get("function")
                    .and_then(Value::as_object)
                    .unwrap_or(object)
            } else {
                object
            };
            let name = function
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| field_error("tools.function.name", "is required"))?;
            let schema = function
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object"}));
            Ok(json!({
                "name": name,
                "description": function.get("description").cloned().unwrap_or(Value::Null),
                "input_schema": schema,
            }))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn reject_common_conversion_fields(body: &Map<String, Value>, chat: bool) -> Result<(), String> {
    if let Some(value) = body.get("n") {
        if value.as_u64() != Some(1) {
            return Err(field_error(
                "n",
                "values other than 1 are unsupported for Anthropic conversion",
            ));
        }
    }
    if body
        .get("response_format")
        .is_some_and(|value| !value.is_null())
    {
        return Err(field_error(
            "response_format",
            "JSON and schema output are unsupported for Anthropic conversion",
        ));
    }
    let reasoning = body
        .get(if chat {
            "reasoning_effort"
        } else {
            "reasoning"
        })
        .or_else(|| body.get("reasoningEffort"));
    if reasoning.is_some_and(|value| match value {
        Value::Null => false,
        Value::String(text) => !text.trim().is_empty(),
        Value::Object(object) => !object.is_empty(),
        _ => true,
    }) {
        return Err(field_error(
            "reasoning_effort",
            "is not translatable to Anthropic Messages",
        ));
    }
    Ok(())
}

fn assistant_tool_blocks(message: &Map<String, Value>) -> Result<Vec<Value>, String> {
    let mut blocks = Vec::new();
    if let Some(content) = message.get("content").filter(|value| !value.is_null()) {
        blocks.extend(chat_content_blocks(content)?);
    }
    for call in message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map_or(&[][..], |calls| calls.as_slice())
    {
        let function = call
            .get("function")
            .ok_or_else(|| "messages.tool_calls.function: field is required".to_string())?;
        let arguments = function
            .get("arguments")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "messages.tool_calls.function.arguments: expected JSON string".to_string()
            })?;
        let input = serde_json::from_str::<Value>(arguments)
            .map_err(|_| "messages.tool_calls.function.arguments: invalid JSON".to_string())?;
        blocks.push(json!({
            "type": "tool_use",
            "id": call.get("id").and_then(Value::as_str).unwrap_or_default(),
            "name": function.get("name").and_then(Value::as_str).unwrap_or_default(),
            "input": input,
        }));
    }
    Ok(blocks)
}

fn convert_chat(body: &Map<String, Value>, model: &str) -> Result<Value, String> {
    reject_common_conversion_fields(body, true)?;
    let max_tokens = output_cap(body, &["max_completion_tokens", "max_tokens"])?;
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .map_or(&[][..], |messages| messages.as_slice());
    let mut system = Vec::new();
    let mut native_messages = Vec::new();
    for message in messages {
        let object = message
            .as_object()
            .ok_or_else(|| "messages: every message must be an object".to_string())?;
        let role = object
            .get("role")
            .and_then(Value::as_str)
            .ok_or_else(|| "messages.role: field is required".to_string())?;
        match role {
            "system" | "developer" => {
                let content = object
                    .get("content")
                    .ok_or_else(|| "messages.content: field is required".to_string())?;
                system.extend(chat_content_blocks(content)?);
            }
            "user" => {
                let content = object
                    .get("content")
                    .ok_or_else(|| "messages.content: field is required".to_string())?;
                native_messages
                    .push(json!({"role": "user", "content": chat_content_blocks(content)?}));
            }
            "assistant" => {
                native_messages.push(json!({
                    "role": "assistant",
                    "content": assistant_tool_blocks(object)?,
                }));
            }
            "tool" => {
                let call_id = object
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .filter(|id| !id.trim().is_empty())
                    .ok_or_else(|| "messages.tool_call_id: field is required".to_string())?;
                let content = object.get("content").cloned().unwrap_or(Value::Null);
                native_messages.push(json!({
                    "role": "user",
                    "content": [{"type": "tool_result", "tool_use_id": call_id, "content": content}],
                }));
            }
            _ => return Err(format!("messages.role: unsupported role '{role}'")),
        }
    }
    let mut result = Map::new();
    result.insert("model".to_string(), Value::String(model.to_string()));
    result.insert("max_tokens".to_string(), json!(max_tokens));
    result.insert("messages".to_string(), Value::Array(native_messages));
    if !system.is_empty() {
        result.insert("system".to_string(), Value::Array(system));
    }
    for key in ["temperature", "top_p"] {
        if let Some(value) = body.get(key) {
            result.insert(key.to_string(), value.clone());
        }
    }
    if let Some(stop) = body.get("stop") {
        result.insert(
            "stop_sequences".to_string(),
            if let Some(text) = stop.as_str() {
                json!([text])
            } else {
                stop.clone()
            },
        );
    }
    if let Some(tools) = convert_tools(body.get("tools"))? {
        result.insert("tools".to_string(), Value::Array(tools));
    }
    if let Some(choice) = tool_choice(body.get("tool_choice"))? {
        if body.get("parallel_tool_calls").and_then(Value::as_bool) == Some(false) {
            let mut choice = choice;
            if let Some(object) = choice.as_object_mut() {
                object.insert("disable_parallel_tool_use".to_string(), Value::Bool(true));
            }
            result.insert("tool_choice".to_string(), choice);
        } else {
            result.insert("tool_choice".to_string(), choice);
        }
    }
    Ok(Value::Object(result))
}

fn append_responses_item(item: &Value, messages: &mut Vec<Value>) -> Result<(), String> {
    let object = item
        .as_object()
        .ok_or_else(|| "input: every item must be an object or string".to_string())?;
    let item_type = object.get("type").and_then(Value::as_str);
    if item_type == Some("item_reference") {
        return Err(field_error(
            "input",
            "unresolved item references are unsupported",
        ));
    }
    if item_type == Some("function_call") {
        let id = object
            .get("call_id")
            .or_else(|| object.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let arguments = object
            .get("arguments")
            .and_then(Value::as_str)
            .ok_or_else(|| "input.function_call.arguments: expected JSON string".to_string())?;
        let input = serde_json::from_str::<Value>(arguments)
            .map_err(|_| "input.function_call.arguments: invalid JSON".to_string())?;
        messages.push(json!({
            "role": "assistant",
            "content": [{"type": "tool_use", "id": id, "name": object.get("name").and_then(Value::as_str).unwrap_or_default(), "input": input}],
        }));
        return Ok(());
    }
    if item_type == Some("function_call_output") {
        let id = object
            .get("call_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "input.function_call_output.call_id: field is required".to_string())?;
        let output = object.get("output").cloned().unwrap_or(Value::Null);
        messages.push(json!({
            "role": "user",
            "content": [{"type": "tool_result", "tool_use_id": id, "content": output}],
        }));
        return Ok(());
    }
    let role = object
        .get("role")
        .and_then(Value::as_str)
        .ok_or_else(|| field_error("input.role", "field is required"))?;
    let content = object
        .get("content")
        .ok_or_else(|| field_error("input.content", "field is required"))?;
    let blocks = response_content_blocks(content)?;
    messages.push(json!({"role": role, "content": blocks}));
    Ok(())
}

fn convert_responses(body: &Map<String, Value>, model: &str) -> Result<Value, String> {
    reject_common_conversion_fields(body, false)?;
    for field in [
        "previous_response_id",
        "conversation",
        "background",
        "store",
        "truncation",
    ] {
        if body.get(field).is_some_and(|value| match field {
            "background" | "store" => value.as_bool() == Some(true),
            _ => !value.is_null(),
        }) {
            return Err(field_error(
                field,
                "is unsupported for stateless Anthropic conversion",
            ));
        }
    }
    let max_tokens = output_cap(body, &["max_output_tokens"])?;
    let mut messages = Vec::new();
    if let Some(instructions) = body.get("instructions") {
        messages.push(json!({"role": "system", "content": response_content_blocks(instructions)?}));
    }
    let input = body
        .get("input")
        .ok_or_else(|| field_error("input", "field is required"))?;
    if let Some(text) = input.as_str() {
        messages.push(json!({"role": "user", "content": [{"type": "text", "text": text}]}));
    } else if let Some(items) = input.as_array() {
        for item in items {
            append_responses_item(item, &mut messages)?;
        }
    } else {
        append_responses_item(input, &mut messages)?;
    }
    let mut result = Map::new();
    result.insert("model".to_string(), Value::String(model.to_string()));
    result.insert("max_tokens".to_string(), json!(max_tokens));
    result.insert("messages".to_string(), Value::Array(messages));
    for key in ["temperature", "top_p"] {
        if let Some(value) = body.get(key) {
            result.insert(key.to_string(), value.clone());
        }
    }
    if let Some(tools) = convert_tools(body.get("tools"))? {
        result.insert("tools".to_string(), Value::Array(tools));
    }
    if let Some(choice) = tool_choice(body.get("tool_choice"))? {
        result.insert("tool_choice".to_string(), choice);
    }
    if body.get("parallel_tool_calls").and_then(Value::as_bool) == Some(false) {
        result.insert("disable_parallel_tool_use".to_string(), Value::Bool(true));
    }
    Ok(Value::Object(result))
}

pub(in crate::ai_gateway) fn convert_request(
    path: &str,
    body: &[u8],
    model: &str,
) -> Result<Vec<u8>, String> {
    let value: Value = serde_json::from_slice(body).map_err(|error| error.to_string())?;
    let object = required_object(&value)?;
    let converted = match path.split('?').next().unwrap_or(path) {
        "/v1/chat/completions" => convert_chat(object, model)?,
        "/v1/responses" => convert_responses(object, model)?,
        _ => return Err("request path is not convertible to Anthropic Messages".to_string()),
    };
    serde_json::to_vec(&converted).map_err(|error| error.to_string())
}

pub(in crate::ai_gateway) fn validate_conversion_request(
    path: &str,
    body: &Value,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(body).map_err(|error| error.to_string())?;
    convert_request(path, &bytes, "validation").map(|_| ())
}

fn anthropic_usage_to_openai(usage: &Value) -> Value {
    let canonical = super::usage_log::canonical_usage_from_value(usage).tokens;
    let input = canonical
        .input_tokens
        .saturating_add(canonical.cache_read_tokens)
        .saturating_add(canonical.cache_write_tokens);
    json!({
        "prompt_tokens": input,
        "completion_tokens": canonical.output_tokens,
        "total_tokens": input.saturating_add(canonical.output_tokens),
        "prompt_tokens_details": {"cached_tokens": canonical.cache_read_tokens},
    })
}

pub(in crate::ai_gateway) fn convert_response(
    path: &str,
    body: &[u8],
    model: &str,
) -> Result<Vec<u8>, String> {
    let value: Value = serde_json::from_slice(body).map_err(|error| error.to_string())?;
    let object = value
        .as_object()
        .ok_or_else(|| "Anthropic response must be a JSON object".to_string())?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("msg_unknown");
    let blocks = object
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| "Anthropic response.content must be an array".to_string())?;
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    let mut output = Vec::new();
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(value) = block.get("text").and_then(Value::as_str) {
                    text.push_str(value);
                    output.push(json!({
                        "type": "output_text",
                        "text": value,
                        "annotations": [],
                    }));
                }
            }
            Some("tool_use") => {
                let tool_id = block.get("id").and_then(Value::as_str).unwrap_or_default();
                let name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let input = block
                    .get("input")
                    .cloned()
                    .unwrap_or(Value::Object(Map::new()));
                let arguments = serde_json::to_string(&input).map_err(|error| error.to_string())?;
                tool_calls.push(json!({
                    "id": tool_id,
                    "type": "function",
                    "function": {"name": name, "arguments": arguments},
                }));
                output.push(json!({
                    "type": "function_call",
                    "id": tool_id,
                    "call_id": tool_id,
                    "name": name,
                    "arguments": arguments,
                    "status": "completed",
                }));
            }
            _ => {}
        }
    }
    let stop_reason = object.get("stop_reason").and_then(Value::as_str);
    let usage = object.get("usage").cloned().unwrap_or_else(|| json!({}));
    let usage_openai = anthropic_usage_to_openai(&usage);
    let payload = if path.split('?').next().unwrap_or(path) == "/v1/chat/completions" {
        let message = json!({
            "role": "assistant",
            "content": if text.is_empty() { Value::Null } else { Value::String(text) },
            "tool_calls": if tool_calls.is_empty() { Value::Null } else { Value::Array(tool_calls) },
        });
        json!({
            "id": id,
            "object": "chat.completion",
            "model": model,
            "choices": [{"index": 0, "message": message, "finish_reason": if stop_reason == Some("tool_use") { "tool_calls" } else { "stop" }}],
            "usage": usage_openai,
        })
    } else {
        let input_tokens = usage_openai
            .get("prompt_tokens")
            .cloned()
            .unwrap_or(json!(0));
        let output_tokens = usage_openai
            .get("completion_tokens")
            .cloned()
            .unwrap_or(json!(0));
        json!({
            "id": id,
            "object": "response",
            "model": model,
            "status": "completed",
            "output": if output.is_empty() { vec![json!({"type":"message","id":format!("{id}_message"),"role":"assistant","status":"completed","content":[]})] } else { output },
            "usage": {"input_tokens": input_tokens, "output_tokens": output_tokens, "total_tokens": usage_openai.get("total_tokens").cloned().unwrap_or(json!(0)), "input_tokens_details": {"cached_tokens": usage_openai.pointer("/prompt_tokens_details/cached_tokens").cloned().unwrap_or(json!(0))}},
        })
    };
    serde_json::to_vec(&payload).map_err(|error| error.to_string())
}
