//! Chat Completions <-> Responses request conversion.

use super::super::UpstreamProtocol;
use super::helpers::{field_error, required_str};
use super::protocol::client_protocol;
use serde_json::{json, Map, Value};

// ---------------------------------------------------------------------------
// Request conversion
// ---------------------------------------------------------------------------

/// Convert a pool request from the client's OpenAI protocol to the selected
/// candidate's OpenAI protocol. `upstream` must differ from the path's protocol;
/// the model is rewritten to the real upstream model.
pub(in crate::ai_gateway) fn convert_request(
    path: &str,
    body: &[u8],
    model: &str,
    upstream: UpstreamProtocol,
) -> Result<Vec<u8>, String> {
    let value: Value = serde_json::from_slice(body).map_err(|error| error.to_string())?;
    let object = value
        .as_object()
        .ok_or_else(|| "request body must be a JSON object".to_string())?;
    let converted = match (client_protocol(path), upstream) {
        (Some(UpstreamProtocol::ChatCompletions), UpstreamProtocol::Responses) => {
            chat_request_to_responses(object, model)?
        }
        (Some(UpstreamProtocol::Responses), UpstreamProtocol::ChatCompletions) => {
            responses_request_to_chat(object, model)?
        }
        _ => return Err("unsupported OpenAI conversion direction".to_string()),
    };
    serde_json::to_vec(&converted).map_err(|error| error.to_string())
}

/// Extract text from a Chat content value (string or text blocks), rejecting
/// image/file/audio and other non-text blocks. Used for message content.
fn chat_content_text(content: Option<&Value>) -> Result<String, String> {
    match content {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(text)) => Ok(text.clone()),
        Some(Value::Array(blocks)) => {
            let mut text = String::new();
            for block in blocks {
                let object = block
                    .as_object()
                    .ok_or_else(|| field_error("messages.content", "invalid content block"))?;
                match object.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        text.push_str(
                            object
                                .get("text")
                                .and_then(Value::as_str)
                                .ok_or_else(|| {
                                    field_error("messages.content.text", "expected a string")
                                })?,
                        );
                    }
                    Some("image_url") | Some("image") | Some("input_image") => {
                        return Err(field_error(
                            "messages.content",
                            "image input is not supported by the model-pool contract",
                        ))
                    }
                    Some("input_audio") | Some("audio") => {
                        return Err(field_error(
                            "messages.content",
                            "audio input is not supported by the model-pool contract",
                        ))
                    }
                    Some(other) => {
                        return Err(field_error(
                            "messages.content",
                            format!("unsupported content block '{other}'"),
                        ))
                    }
                    None => return Err(field_error("messages.content", "type is required")),
                }
            }
            Ok(text)
        }
        Some(_) => Err(field_error(
            "messages.content",
            "must be text or content blocks",
        )),
    }
}

fn chat_request_to_responses(body: &Map<String, Value>, model: &str) -> Result<Value, String> {
    let mut out = Map::new();
    out.insert("model".to_string(), Value::String(model.to_string()));
    let mut instructions: Vec<String> = Vec::new();
    let mut input: Vec<Value> = Vec::new();
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .map_or(&[][..], |messages| messages.as_slice());
    for message in messages {
        let object = message
            .as_object()
            .ok_or_else(|| field_error("messages", "every message must be an object"))?;
        let role = object
            .get("role")
            .and_then(Value::as_str)
            .ok_or_else(|| field_error("messages.role", "field is required"))?;
        match role {
            "system" | "developer" => {
                let text = chat_content_text(object.get("content"))?;
                if !text.trim().is_empty() {
                    instructions.push(text);
                }
            }
            "user" => {
                let text = chat_content_text(object.get("content"))?;
                input.push(json!({
                    "role": "user",
                    "content": [{"type": "input_text", "text": text}],
                }));
            }
            "assistant" => {
                if let Some(content) = object.get("content").filter(|value| !value.is_null()) {
                    let text = chat_content_text(Some(content))?;
                    if !text.is_empty() {
                        input.push(json!({
                            "role": "assistant",
                            "content": [{"type": "output_text", "text": text}],
                        }));
                    }
                }
                for call in object
                    .get("tool_calls")
                    .and_then(Value::as_array)
                    .map_or(&[][..], |calls| calls.as_slice())
                {
                    let id = required_str(call.get("id"), "messages.tool_calls.id")?;
                    let function = call
                        .get("function")
                        .ok_or_else(|| field_error("messages.tool_calls.function", "is required"))?;
                    let name = required_str(
                        function.get("name"),
                        "messages.tool_calls.function.name",
                    )?;
                    let arguments = required_str(
                        function.get("arguments"),
                        "messages.tool_calls.function.arguments",
                    )?;
                    serde_json::from_str::<Value>(arguments).map_err(|_| {
                        field_error(
                            "messages.tool_calls.function.arguments",
                            "must be a JSON string",
                        )
                    })?;
                    input.push(json!({
                        "type": "function_call",
                        "call_id": id,
                        "name": name,
                        "arguments": arguments,
                    }));
                }
            }
            "tool" => {
                let call_id = required_str(object.get("tool_call_id"), "messages.tool_call_id")?;
                let output = chat_content_text(object.get("content"))?;
                input.push(json!({
                    "type": "function_call_output",
                    "call_id": call_id,
                    "output": output,
                }));
            }
            other => {
                return Err(field_error(
                    "messages.role",
                    format!("unsupported role '{other}'"),
                ))
            }
        }
    }
    if !instructions.is_empty() {
        out.insert(
            "instructions".to_string(),
            Value::String(instructions.join("\n")),
        );
    }
    out.insert("input".to_string(), Value::Array(input));
    if let Some(tools) = body.get("tools") {
        out.insert("tools".to_string(), chat_tools_to_responses(tools)?);
    }
    if let Some(choice) = body.get("tool_choice").filter(|value| !value.is_null()) {
        out.insert("tool_choice".to_string(), chat_tool_choice_to_responses(choice)?);
    }
    if let Some(parallel) = body.get("parallel_tool_calls") {
        out.insert("parallel_tool_calls".to_string(), parallel.clone());
    }
    for key in ["temperature", "top_p"] {
        if let Some(value) = body.get(key) {
            out.insert(key.to_string(), value.clone());
        }
    }
    if let Some(limit) = chat_output_limit(body)? {
        out.insert("max_output_tokens".to_string(), limit);
    }
    for key in ["stream", "store", "background"] {
        if let Some(value) = body.get(key).filter(|value| !value.is_null()) {
            out.insert(key.to_string(), value.clone());
        }
    }
    Ok(Value::Object(out))
}

/// Validate every supplied Chat output limit independently, then apply
/// `max_completion_tokens` precedence over `max_tokens`. An omitted limit stays
/// omitted; a supplied limit must be a non-negative integer (zero allowed).
fn chat_output_limit(body: &Map<String, Value>) -> Result<Option<Value>, String> {
    let mut chosen: Option<u64> = None;
    for field in ["max_completion_tokens", "max_tokens"] {
        if let Some(value) = body.get(field).filter(|value| !value.is_null()) {
            let limit = value
                .as_u64()
                .ok_or_else(|| field_error(field, "must be a non-negative integer"))?;
            if chosen.is_none() {
                chosen = Some(limit);
            }
        }
    }
    Ok(chosen.map(|limit| json!(limit)))
}

fn chat_tool_property(function: &Value, key: &str) -> Value {
    function.get(key).cloned().unwrap_or(Value::Null)
}

fn chat_tools_to_responses(tools: &Value) -> Result<Value, String> {
    let tools = tools
        .as_array()
        .ok_or_else(|| field_error("tools", "must be an array"))?;
    let converted = tools
        .iter()
        .map(|tool| {
            let tool_type = tool
                .get("type")
                .and_then(Value::as_str)
                .ok_or_else(|| field_error("tools", "every tool requires a type"))?;
            if tool_type != "function" {
                return Err(field_error(
                    "tools",
                    format!("hosted tool '{tool_type}' is not supported"),
                ));
            }
            let function = tool
                .get("function")
                .ok_or_else(|| field_error("tools.function", "is required"))?;
            let name = required_str(function.get("name"), "tools.function.name")?;
            let mut converted = json!({
                "type": "function",
                "name": name,
                "description": chat_tool_property(function, "description"),
                "parameters": function
                    .get("parameters")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "object"})),
            });
            if let Some(strict) = function.get("strict").filter(|value| !value.is_null()) {
                converted["strict"] = strict.clone();
            }
            Ok(converted)
        })
        .collect::<Result<Vec<Value>, String>>()?;
    Ok(Value::Array(converted))
}

fn chat_tool_choice_to_responses(choice: &Value) -> Result<Value, String> {
    if let Some(text) = choice.as_str() {
        return Ok(Value::String(text.to_string()));
    }
    let object = choice
        .as_object()
        .ok_or_else(|| field_error("tool_choice", "must be a string or object"))?;
    match object.get("type").and_then(Value::as_str) {
        Some("function") => Ok(json!({
            "type": "function",
            "name": object
                .get("function")
                .and_then(|function| function.get("name"))
                .and_then(Value::as_str)
                .ok_or_else(|| field_error("tool_choice.function.name", "is required"))?,
        })),
        Some(other) => Ok(json!({"type": other})),
        None => Err(field_error("tool_choice.type", "is required")),
    }
}

fn responses_content_text(content: &Value) -> Result<String, String> {
    match content {
        Value::Null => Ok(String::new()),
        Value::String(text) => Ok(text.clone()),
        Value::Array(blocks) => {
            let mut text = String::new();
            for block in blocks {
                let object = block
                    .as_object()
                    .ok_or_else(|| field_error("input.content", "invalid content block"))?;
                match object.get("type").and_then(Value::as_str) {
                    Some("input_text") | Some("output_text") | Some("text") => {
                        text.push_str(
                            object
                                .get("text")
                                .and_then(Value::as_str)
                                .ok_or_else(|| {
                                    field_error("input.content.text", "expected a string")
                                })?,
                        );
                    }
                    Some("input_image") => {
                        return Err(field_error(
                            "image",
                            "image input is not supported by the model-pool contract",
                        ))
                    }
                    Some("input_file") => {
                        return Err(field_error(
                            "file",
                            "file input is not supported by the model-pool contract",
                        ))
                    }
                    Some("input_audio") => {
                        return Err(field_error(
                            "audio",
                            "audio input is not supported by the model-pool contract",
                        ))
                    }
                    Some(other) => {
                        return Err(field_error(
                            "input.content",
                            format!("unsupported content block '{other}'"),
                        ))
                    }
                    None => return Err(field_error("input.content", "type is required")),
                }
            }
            Ok(text)
        }
        _ => Err(field_error("input.content", "must be text or content blocks")),
    }
}

fn responses_request_to_chat(body: &Map<String, Value>, model: &str) -> Result<Value, String> {
    let mut messages: Vec<Value> = Vec::new();
    if let Some(instructions) = body.get("instructions").filter(|value| !value.is_null()) {
        let text = responses_content_text(instructions)?;
        if !text.trim().is_empty() {
            messages.push(json!({"role": "system", "content": text}));
        }
    }
    match body.get("input") {
        None => return Err(field_error("input", "field is required")),
        Some(Value::String(text)) => messages.push(json!({"role": "user", "content": text})),
        Some(Value::Array(items)) => {
            for item in items {
                responses_item_to_chat(item, &mut messages)?;
            }
        }
        Some(item @ Value::Object(_)) => responses_item_to_chat(item, &mut messages)?,
        Some(_) => return Err(field_error("input", "must be a string or an array")),
    }
    let mut out = Map::new();
    out.insert("model".to_string(), Value::String(model.to_string()));
    out.insert("messages".to_string(), Value::Array(messages));
    if let Some(tools) = body.get("tools") {
        out.insert("tools".to_string(), responses_tools_to_chat(tools)?);
    }
    if let Some(choice) = body.get("tool_choice").filter(|value| !value.is_null()) {
        out.insert("tool_choice".to_string(), responses_tool_choice_to_chat(choice)?);
    }
    if let Some(parallel) = body.get("parallel_tool_calls") {
        out.insert("parallel_tool_calls".to_string(), parallel.clone());
    }
    for key in ["temperature", "top_p"] {
        if let Some(value) = body.get(key) {
            out.insert(key.to_string(), value.clone());
        }
    }
    if let Some(limit) = body.get("max_output_tokens").filter(|value| !value.is_null()) {
        let limit = limit
            .as_u64()
            .ok_or_else(|| field_error("max_output_tokens", "must be a non-negative integer"))?;
        out.insert("max_tokens".to_string(), json!(limit));
    }
    for key in ["stream", "store", "background"] {
        if let Some(value) = body.get(key).filter(|value| !value.is_null()) {
            out.insert(key.to_string(), value.clone());
        }
    }
    Ok(Value::Object(out))
}

fn responses_item_to_chat(item: &Value, messages: &mut Vec<Value>) -> Result<(), String> {
    let object = item
        .as_object()
        .ok_or_else(|| field_error("input", "every item must be an object"))?;
    match object.get("type").and_then(Value::as_str) {
        Some("function_call") => {
            let arguments = required_str(
                object.get("arguments"),
                "input.function_call.arguments",
            )?;
            let name = required_str(object.get("name"), "input.function_call.name")?;
            let call_id = object
                .get("call_id")
                .or_else(|| object.get("id"))
                .and_then(Value::as_str)
                .filter(|id| !id.trim().is_empty())
                .ok_or_else(|| field_error("input.function_call.call_id", "is required"))?;
            messages.push(json!({
                "role": "assistant",
                "content": Value::Null,
                "tool_calls": [{
                    "id": call_id,
                    "type": "function",
                    "function": {"name": name, "arguments": arguments},
                }],
            }));
            Ok(())
        }
        Some("function_call_output") => {
            let call_id = required_str(
                object.get("call_id"),
                "input.function_call_output.call_id",
            )?;
            let output = responses_content_text(object.get("output").unwrap_or(&Value::Null))?;
            messages.push(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": output,
            }));
            Ok(())
        }
        Some("item_reference") => Err(field_error(
            "item_reference",
            "unresolved item references are not supported by the model-pool contract",
        )),
        Some("reasoning") => Err(field_error(
            "reasoning",
            "opaque reasoning state is not supported by the model-pool contract",
        )),
        Some("message") | None => {
            let role = object
                .get("role")
                .and_then(Value::as_str)
                .ok_or_else(|| field_error("input.role", "field is required"))?;
            let content = object
                .get("content")
                .ok_or_else(|| field_error("input.content", "field is required"))?;
            let text = responses_content_text(content)?;
            messages.push(json!({"role": role, "content": text}));
            Ok(())
        }
        Some(other) => Err(field_error(
            other,
            "is not supported by the model-pool contract",
        )),
    }
}

fn responses_tools_to_chat(tools: &Value) -> Result<Value, String> {
    let tools = tools
        .as_array()
        .ok_or_else(|| field_error("tools", "must be an array"))?;
    let converted = tools
        .iter()
        .map(|tool| {
            let tool_type = tool
                .get("type")
                .and_then(Value::as_str)
                .ok_or_else(|| field_error("tools", "every tool requires a type"))?;
            if tool_type != "function" {
                return Err(field_error(
                    "tools",
                    format!("hosted tool '{tool_type}' is not supported"),
                ));
            }
            let name = required_str(tool.get("name"), "tools.name")?;
            let mut function = json!({
                "name": name,
                "description": tool.get("description").cloned().unwrap_or(Value::Null),
                "parameters": tool
                    .get("parameters")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "object"})),
            });
            if let Some(strict) = tool.get("strict").filter(|value| !value.is_null()) {
                function["strict"] = strict.clone();
            }
            Ok(json!({"type": "function", "function": function}))
        })
        .collect::<Result<Vec<Value>, String>>()?;
    Ok(Value::Array(converted))
}

fn responses_tool_choice_to_chat(choice: &Value) -> Result<Value, String> {
    if let Some(text) = choice.as_str() {
        return Ok(Value::String(text.to_string()));
    }
    let object = choice
        .as_object()
        .ok_or_else(|| field_error("tool_choice", "must be a string or object"))?;
    match object.get("type").and_then(Value::as_str) {
        Some("function") => Ok(json!({
            "type": "function",
            "function": {
                "name": object
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| field_error("tool_choice.name", "is required"))?,
            },
        })),
        Some(other) => Ok(json!({"type": other})),
        None => Err(field_error("tool_choice.type", "is required")),
    }
}
