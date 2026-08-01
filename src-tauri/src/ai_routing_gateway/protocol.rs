use serde_json::{json, Map, Value};
use std::collections::HashSet;

use super::types::UpstreamProtocol;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProtocolError {
    pub(crate) code: &'static str,
    pub(crate) message: &'static str,
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
    let raw = std::str::from_utf8(bytes).map_err(|_| invalid_request())?;
    let mut output = String::new();
    for block in raw.split("\n\n") {
        let data = block
            .lines()
            .find_map(|line| line.strip_prefix("data:").map(str::trim));
        let Some(data) = data else { continue };
        if data == "[DONE]" {
            output.push_str("data: [DONE]\n\n");
            continue;
        }
        let value: Value = serde_json::from_str(data).map_err(|_| invalid_request())?;
        for event in convert_stream_event(source, target, &value, public_model)? {
            output.push_str("data: ");
            output.push_str(&event.to_string());
            output.push_str("\n\n");
        }
    }
    Ok(output.into_bytes())
}

fn chat_to_responses_request(input: &Value, model: &str) -> Result<Value, ProtocolError> {
    let mut output = Map::new();
    output.insert("model".into(), Value::String(model.into()));
    output.insert(
        "input".into(),
        input.get("messages").cloned().ok_or_else(invalid_request)?,
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
    Ok(Value::Object(output))
}

fn responses_to_chat_request(input: &Value, model: &str) -> Result<Value, ProtocolError> {
    let input_value = input.get("input").ok_or_else(invalid_request)?;
    let messages = match input_value {
        Value::String(text) => json!([{ "role": "user", "content": text }]),
        Value::Array(_) => input_value.clone(),
        _ => return Err(lossless()),
    };
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
        let effort = reasoning.get("effort").cloned().ok_or_else(lossless)?;
        output.insert("reasoning_effort".into(), effort);
    }
    Ok(Value::Object(output))
}

fn responses_to_chat_response(value: &Value, model: &str) -> Result<Value, ProtocolError> {
    if value.get("error").is_some() {
        return Ok(normalize_error(value));
    }
    let mut content = String::new();
    let mut tool_calls = Vec::new();
    let mut reasoning = String::new();
    for item in value
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(lossless)?
    {
        match item.get("type").and_then(Value::as_str) {
            Some("message") => {
                for block in item
                    .get("content")
                    .and_then(Value::as_array)
                    .ok_or_else(lossless)?
                {
                    match block.get("type").and_then(Value::as_str) {
                        Some("output_text") => content.push_str(
                            block.get("text").and_then(Value::as_str).ok_or_else(lossless)?,
                        ),
                        Some("reasoning_text") => reasoning.push_str(
                            block.get("text").and_then(Value::as_str).ok_or_else(lossless)?,
                        ),
                        _ => return Err(lossless()),
                    }
                }
            }
            Some("function_call") => tool_calls.push(json!({
                "id": item.get("call_id").or_else(|| item.get("id")).cloned().unwrap_or(Value::Null),
                "type": "function",
                "function": {
                    "name": item.get("name").cloned().ok_or_else(lossless)?,
                    "arguments": item.get("arguments").cloned().unwrap_or_else(|| Value::String("{}".into()))
                }
            })),
            Some("reasoning") => reasoning.push_str(
                item.get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
            _ => return Err(lossless()),
        }
    }
    let mut message = json!({ "role": "assistant", "content": content });
    if !tool_calls.is_empty() {
        message["tool_calls"] = Value::Array(tool_calls);
    }
    if !reasoning.is_empty() {
        message["reasoning_content"] = Value::String(reasoning);
    }
    let finish = if message.get("tool_calls").is_some() {
        "tool_calls"
    } else if value.get("status").and_then(Value::as_str) == Some("incomplete") {
        "length"
    } else {
        "stop"
    };
    Ok(json!({
        "id": value.get("id").cloned().unwrap_or_else(|| Value::String(format!("chatcmpl_{}", uuid::Uuid::new_v4().simple()))),
        "object": "chat.completion", "model": model,
        "choices": [{ "index": 0, "message": message, "finish_reason": finish }],
        "usage": responses_usage_to_chat(value.get("usage"))
    }))
}

fn chat_to_responses_response(value: &Value, model: &str) -> Result<Value, ProtocolError> {
    if value.get("error").is_some() {
        return Ok(normalize_error(value));
    }
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or_else(lossless)?;
    let message = choice.get("message").ok_or_else(lossless)?;
    let mut output = Vec::new();
    let mut content = Vec::new();
    if let Some(text) = message.get("content").and_then(Value::as_str) {
        content.push(json!({ "type": "output_text", "text": text, "annotations": [] }));
    }
    if let Some(reasoning) = message.get("reasoning_content").and_then(Value::as_str) {
        content.push(json!({ "type": "reasoning_text", "text": reasoning }));
    }
    if !content.is_empty() {
        output.push(json!({ "type": "message", "role": "assistant", "content": content }));
    }
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for call in calls {
            let function = call.get("function").ok_or_else(lossless)?;
            output.push(json!({
                "type": "function_call", "id": call.get("id").cloned().unwrap_or(Value::Null),
                "call_id": call.get("id").cloned().unwrap_or(Value::Null),
                "name": function.get("name").cloned().ok_or_else(lossless)?,
                "arguments": function.get("arguments").cloned().unwrap_or_else(|| Value::String("{}".into()))
            }));
        }
    }
    let finish = choice.get("finish_reason").and_then(Value::as_str);
    Ok(json!({
        "id": value.get("id").cloned().unwrap_or_else(|| Value::String(format!("resp_{}", uuid::Uuid::new_v4().simple()))),
        "object": "response", "model": model,
        "status": if finish == Some("length") { "incomplete" } else { "completed" },
        "output": output, "usage": chat_usage_to_responses(value.get("usage"))
    }))
}

fn convert_stream_event(
    source: UpstreamProtocol,
    target: UpstreamProtocol,
    value: &Value,
    model: &str,
) -> Result<Vec<Value>, ProtocolError> {
    match (source, target) {
        (UpstreamProtocol::ChatCompletions, UpstreamProtocol::Responses) => {
            let choice = value
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|v| v.first());
            if let Some(text) = choice
                .and_then(|v| v.get("delta"))
                .and_then(|v| v.get("content"))
                .and_then(Value::as_str)
            {
                return Ok(vec![
                    json!({ "type": "response.output_text.delta", "delta": text, "model": model }),
                ]);
            }
            if let Some(calls) = choice
                .and_then(|v| v.get("delta"))
                .and_then(|v| v.get("tool_calls"))
                .and_then(Value::as_array)
            {
                return Ok(calls.iter().map(|call| json!({ "type": "response.function_call_arguments.delta", "item_id": call.get("id"), "delta": call.get("function").and_then(|v| v.get("arguments")).cloned().unwrap_or(Value::String(String::new())) })).collect());
            }
            if value.get("usage").is_some() {
                return Ok(vec![
                    json!({ "type": "response.completed", "response": { "model": model, "usage": chat_usage_to_responses(value.get("usage")) } }),
                ]);
            }
            Ok(Vec::new())
        }
        (UpstreamProtocol::Responses, UpstreamProtocol::ChatCompletions) => {
            match value.get("type").and_then(Value::as_str) {
                Some("response.output_text.delta") => Ok(vec![chat_chunk(
                    model,
                    json!({ "content": value.get("delta").cloned().unwrap_or(Value::String(String::new())) }),
                    Value::Null,
                    None,
                )]),
                Some("response.function_call_arguments.delta") => Ok(vec![chat_chunk(
                    model,
                    json!({ "tool_calls": [{ "index": 0, "id": value.get("item_id"), "type": "function", "function": { "arguments": value.get("delta") } }] }),
                    Value::Null,
                    None,
                )]),
                Some("response.reasoning_summary_text.delta") => Ok(vec![chat_chunk(
                    model,
                    json!({ "reasoning_content": value.get("delta") }),
                    Value::Null,
                    None,
                )]),
                Some("response.completed") => Ok(vec![chat_chunk(
                    model,
                    json!({}),
                    Value::String("stop".into()),
                    value.get("response").and_then(|v| v.get("usage")),
                )]),
                Some("error") => Ok(vec![normalize_error(value)]),
                _ => Ok(Vec::new()),
            }
        }
        _ => unreachable!(),
    }
}

fn chat_chunk(model: &str, delta: Value, finish_reason: Value, usage: Option<&Value>) -> Value {
    json!({
        "id": format!("chatcmpl_{}", uuid::Uuid::new_v4().simple()), "object": "chat.completion.chunk", "model": model,
        "choices": [{ "index": 0, "delta": delta, "finish_reason": finish_reason }],
        "usage": usage.map(|value| responses_usage_to_chat(Some(value))).unwrap_or(Value::Null)
    })
}

fn normalize_error(value: &Value) -> Value {
    let error = value.get("error").unwrap_or(value);
    error_envelope(
        error
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("upstream_unavailable"),
        error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Upstream request failed"),
    )
}

fn responses_usage_to_chat(usage: Option<&Value>) -> Value {
    let usage = usage.unwrap_or(&Value::Null);
    let input = usage.get("input_tokens").cloned().unwrap_or(Value::Null);
    let output = usage.get("output_tokens").cloned().unwrap_or(Value::Null);
    json!({ "prompt_tokens": input, "completion_tokens": output, "total_tokens": usage.get("total_tokens").cloned().unwrap_or_else(|| sum_numbers(&input, &output)) })
}

fn chat_usage_to_responses(usage: Option<&Value>) -> Value {
    let usage = usage.unwrap_or(&Value::Null);
    let input = usage.get("prompt_tokens").cloned().unwrap_or(Value::Null);
    let output = usage
        .get("completion_tokens")
        .cloned()
        .unwrap_or(Value::Null);
    json!({ "input_tokens": input, "output_tokens": output, "total_tokens": usage.get("total_tokens").cloned().unwrap_or_else(|| sum_numbers(&input, &output)) })
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
            let function = tool.get("function").ok_or_else(lossless)?;
            Ok(json!({
                "type": "function",
                "name": function.get("name").cloned().ok_or_else(lossless)?,
                "description": function.get("description").cloned().unwrap_or(Value::Null),
                "parameters": function.get("parameters").cloned().unwrap_or_else(|| json!({ "type": "object", "properties": {} }))
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
            Ok(json!({
                "type": "function",
                "function": {
                    "name": tool.get("name").cloned().ok_or_else(lossless)?,
                    "description": tool.get("description").cloned().unwrap_or(Value::Null),
                    "parameters": tool.get("parameters").cloned().unwrap_or_else(|| json!({ "type": "object", "properties": {} }))
                }
            }))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

fn chat_tool_choice_to_responses(choice: &Value) -> Result<Value, ProtocolError> {
    if choice.is_string() {
        return Ok(choice.clone());
    }
    let name = choice
        .get("function")
        .and_then(|function| function.get("name"))
        .cloned()
        .ok_or_else(lossless)?;
    Ok(json!({ "type": "function", "name": name }))
}

fn responses_tool_choice_to_chat(choice: &Value) -> Result<Value, ProtocolError> {
    if choice.is_string() {
        return Ok(choice.clone());
    }
    let name = choice.get("name").cloned().ok_or_else(lossless)?;
    Ok(json!({ "type": "function", "function": { "name": name } }))
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
            "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"call_1\",\"delta\":\"{}\"}\n\n",
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
}
