//! Portable request validation for the model-pool contract.

use super::super::UpstreamProtocol;
use super::helpers::{field_error, nonempty, present_nonnull, required_str, validate_bool};
use super::protocol::client_protocol;
use super::request::convert_request;
use serde_json::{Map, Value};

// ---------------------------------------------------------------------------
// Portable-field contract
// ---------------------------------------------------------------------------

/// Top-level Chat Completions fields the portable pool contract understands.
const CHAT_ALLOWED: &[&str] = &[
    "model",
    "messages",
    "stream",
    "stream_options",
    "tools",
    "tool_choice",
    "parallel_tool_calls",
    "temperature",
    "top_p",
    "max_tokens",
    "max_completion_tokens",
    "response_format",
    "n",
    "reasoning_effort",
    "reasoningEffort",
    "store",
    "background",
];

/// Top-level Responses fields the portable pool contract understands.
const RESPONSES_ALLOWED: &[&str] = &[
    "model",
    "instructions",
    "input",
    "stream",
    "tools",
    "tool_choice",
    "parallel_tool_calls",
    "temperature",
    "top_p",
    "max_output_tokens",
    "text",
    "store",
    "background",
    "n",
    "reasoning",
];

/// Validate a pool request against the portable contract before any candidate
/// is selected. A field is either translated faithfully by [`convert_request`]
/// or rejected by name. The one recognized exception is the historical
/// assistant `messages[].reasoning_content` trace (a string or `null`): it is a
/// provider-specific reasoning trace rather than a caller request control, so a
/// same-protocol Chat candidate keeps it exactly and a converted request omits
/// it instead of fabricating reasoning.
///
/// `messages_candidate` is `true` when the pool stores at least one candidate
/// whose effective protocol is native Anthropic Messages. Only then must the
/// request also be representable by the existing Messages adapter (and a
/// `strict: true` function control, which that adapter cannot carry, is
/// rejected regardless of which candidate the rotation would pick).
pub(in crate::ai_gateway) fn validate_pool_request(
    path: &str,
    body: &Value,
    messages_candidate: bool,
) -> Result<(), String> {
    let Some(object) = body.as_object() else {
        return Err("request body must be a JSON object".to_string());
    };
    let chat = match client_protocol(path) {
        Some(UpstreamProtocol::Responses) => {
            validate_responses(object)?;
            false
        }
        Some(UpstreamProtocol::ChatCompletions) => {
            validate_chat(object)?;
            true
        }
        _ => return Ok(()),
    };
    if messages_candidate {
        reject_messages_unrepresentable(chat, object)?;
    }
    // Representability in the opposite OpenAI direction: a pool may contain a
    // candidate on either endpoint, so a request that cannot be represented on
    // the other endpoint is rejected here regardless of which candidate the
    // rotation happens to pick.
    let opposite = if chat {
        UpstreamProtocol::Responses
    } else {
        UpstreamProtocol::ChatCompletions
    };
    let bytes = serde_json::to_vec(body).map_err(|error| error.to_string())?;
    convert_request(path, &bytes, "validation", opposite)?;
    if messages_candidate {
        // A pool may also select a native Anthropic Messages candidate, which
        // reuses the existing adapter's supported-field validation and default.
        super::super::anthropic::validate_conversion_request(path, body)?;
    }
    Ok(())
}

/// Reject a `strict: true` function control when the pool may select a native
/// Messages candidate, because the existing adapter silently drops `strict`.
/// `strict: false` is the representable default and stays accepted.
fn reject_messages_unrepresentable(chat: bool, object: &Map<String, Value>) -> Result<(), String> {
    let Some(tools) = object.get("tools").and_then(Value::as_array) else {
        return Ok(());
    };
    for tool in tools {
        let strict = if chat {
            tool.get("function").and_then(|function| function.get("strict"))
        } else {
            tool.get("strict")
        };
        if strict.and_then(Value::as_bool) == Some(true) {
            return Err(field_error(
                "strict",
                "is not representable by the selected Messages candidate",
            ));
        }
    }
    Ok(())
}

fn validate_chat(object: &Map<String, Value>) -> Result<(), String> {
    for field in [
        "stop",
        "frequency_penalty",
        "presence_penalty",
        "seed",
        "logit_bias",
        "logprobs",
    ] {
        if present_nonnull(object, field) {
            return Err(field_error(
                field,
                "is not supported by the model-pool contract",
            ));
        }
    }
    if let Some(n) = object.get("n") {
        if n.as_u64() != Some(1) {
            return Err(field_error(
                "n",
                "values other than 1 are unsupported by the model-pool contract",
            ));
        }
    }
    if let Some(format) = object.get("response_format").filter(|value| !value.is_null()) {
        validate_chat_response_format(format)?;
    }
    if let Some(options) = object.get("stream_options").filter(|value| !value.is_null()) {
        validate_stream_options(options)?;
    }
    for field in ["reasoning_effort", "reasoningEffort"] {
        if object.get(field).is_some_and(nonempty) {
            return Err(field_error(
                field,
                "reasoning control is not supported by the model-pool contract",
            ));
        }
    }
    for key in object.keys() {
        if !CHAT_ALLOWED.contains(&key.as_str()) {
            return Err(field_error(
                key,
                "is not supported by the model-pool contract",
            ));
        }
    }
    for field in ["stream", "parallel_tool_calls"] {
        validate_bool(object, field)?;
    }
    for field in ["store", "background"] {
        validate_bool(object, field)?;
        if object.get(field).and_then(Value::as_bool) == Some(true) {
            return Err(field_error(
                field,
                "true is not supported by the model-pool contract",
            ));
        }
    }
    if let Some(messages) = object.get("messages").filter(|value| !value.is_null()) {
        let messages = messages
            .as_array()
            .ok_or_else(|| field_error("messages", "must be an array"))?;
        for message in messages {
            validate_chat_message(message)?;
        }
    }
    if let Some(tools) = object.get("tools").filter(|value| !value.is_null()) {
        validate_chat_tools(tools)?;
    }
    if let Some(choice) = object.get("tool_choice").filter(|value| !value.is_null()) {
        validate_chat_tool_choice(choice)?;
    }
    Ok(())
}

fn validate_stream_options(options: &Value) -> Result<(), String> {
    let options = options
        .as_object()
        .ok_or_else(|| field_error("stream_options", "must be an object"))?;
    for key in options.keys() {
        if key != "include_usage" {
            return Err(field_error(
                key,
                "is not a supported stream_options member for the model-pool contract",
            ));
        }
    }
    if let Some(include_usage) = options.get("include_usage") {
        if !include_usage.is_boolean() {
            return Err(field_error("include_usage", "must be a boolean"));
        }
    }
    Ok(())
}

fn validate_chat_response_format(format: &Value) -> Result<(), String> {
    let object = format
        .as_object()
        .ok_or_else(|| field_error("response_format", "must be an object"))?;
    for key in object.keys() {
        if key != "type" {
            return Err(field_error(
                format!("response_format.{key}"),
                "is not supported by the model-pool contract",
            ));
        }
    }
    if object.get("type").and_then(Value::as_str) != Some("text") {
        return Err(field_error(
            "response_format",
            "only text output is supported by the model-pool contract",
        ));
    }
    Ok(())
}

fn validate_chat_message(message: &Value) -> Result<(), String> {
    let object = message
        .as_object()
        .ok_or_else(|| field_error("messages", "every message must be an object"))?;
    for key in object.keys() {
        // `name` has no faithful representation in the opposite Responses
        // message shape, so it is not part of the portable Chat message
        // contract; it must be rejected rather than silently dropped.
        if !matches!(
            key.as_str(),
            "role" | "content" | "tool_calls" | "tool_call_id" | "reasoning_content"
        ) {
            return Err(field_error(
                format!("messages.{key}"),
                "is not supported by the model-pool contract",
            ));
        }
    }
    let role = required_str(object.get("role"), "messages.role")?;
    // The historical assistant `reasoning_content` trace is the one
    // provider-specific field the contract recognizes: assistant-only, a string
    // (including empty or whitespace) or `null`, never a control surface.
    if let Some(reasoning) = object.get("reasoning_content") {
        if role != "assistant" {
            return Err(field_error(
                "messages.reasoning_content",
                "is only supported on assistant messages",
            ));
        }
        if !reasoning.is_null() && !reasoning.is_string() {
            return Err(field_error(
                "messages.reasoning_content",
                "must be a string or null",
            ));
        }
    }
    match role {
        "system" | "developer" | "user" => {
            if let Some(content) = object.get("content").filter(|value| !value.is_null()) {
                validate_chat_content(content, "messages.content")?;
            }
        }
        "assistant" => {
            if let Some(content) = object.get("content").filter(|value| !value.is_null()) {
                validate_chat_content(content, "messages.content")?;
            }
            if let Some(calls) = object.get("tool_calls").filter(|value| !value.is_null()) {
                let calls = calls
                    .as_array()
                    .ok_or_else(|| field_error("messages.tool_calls", "must be an array"))?;
                for call in calls {
                    validate_chat_tool_call(call)?;
                }
            }
        }
        "tool" => {
            required_str(object.get("tool_call_id"), "messages.tool_call_id")?;
            if let Some(content) = object.get("content").filter(|value| !value.is_null()) {
                validate_chat_content(content, "messages.content")?;
            }
        }
        other => {
            return Err(field_error(
                "messages.role",
                format!("unsupported role '{other}'"),
            ))
        }
    }
    Ok(())
}

fn validate_chat_content(content: &Value, field: &str) -> Result<(), String> {
    match content {
        Value::String(_) => Ok(()),
        Value::Array(blocks) => {
            for block in blocks {
                let object = block
                    .as_object()
                    .ok_or_else(|| field_error(field, "invalid content block"))?;
                for key in object.keys() {
                    if !matches!(key.as_str(), "type" | "text") {
                        return Err(field_error(
                            format!("{field}.{key}"),
                            "is not supported by the model-pool contract",
                        ));
                    }
                }
                match object.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if object.get("text").and_then(Value::as_str).is_none() {
                            return Err(field_error(
                                format!("{field}.text"),
                                "expected a string",
                            ));
                        }
                    }
                    Some("image_url") | Some("image") | Some("input_image") => {
                        return Err(field_error(
                            field,
                            "image input is not supported by the model-pool contract",
                        ))
                    }
                    Some("input_file") | Some("file") => {
                        return Err(field_error(
                            field,
                            "file input is not supported by the model-pool contract",
                        ))
                    }
                    Some("input_audio") | Some("audio") => {
                        return Err(field_error(
                            field,
                            "audio input is not supported by the model-pool contract",
                        ))
                    }
                    Some(other) => {
                        return Err(field_error(
                            field,
                            format!("unsupported content block '{other}'"),
                        ))
                    }
                    None => return Err(field_error(format!("{field}.type"), "is required")),
                }
            }
            Ok(())
        }
        _ => Err(field_error(field, "must be text or content blocks")),
    }
}

fn validate_chat_tool_call(call: &Value) -> Result<(), String> {
    let object = call
        .as_object()
        .ok_or_else(|| field_error("messages.tool_calls", "every call must be an object"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "id" | "type" | "function") {
            return Err(field_error(
                format!("messages.tool_calls.{key}"),
                "is not supported by the model-pool contract",
            ));
        }
    }
    required_str(object.get("id"), "messages.tool_calls.id")?;
    if object.get("type").and_then(Value::as_str) != Some("function") {
        return Err(field_error(
            "messages.tool_calls.type",
            "only function tool calls are supported",
        ));
    }
    let function = object
        .get("function")
        .and_then(Value::as_object)
        .ok_or_else(|| field_error("messages.tool_calls.function", "is required"))?;
    for key in function.keys() {
        if !matches!(key.as_str(), "name" | "arguments") {
            return Err(field_error(
                format!("messages.tool_calls.function.{key}"),
                "is not supported by the model-pool contract",
            ));
        }
    }
    required_str(function.get("name"), "messages.tool_calls.function.name")?;
    let arguments = required_str(
        function.get("arguments"),
        "messages.tool_calls.function.arguments",
    )?;
    if serde_json::from_str::<Value>(arguments).is_err() {
        return Err(field_error(
            "messages.tool_calls.function.arguments",
            "must be a JSON string",
        ));
    }
    Ok(())
}

fn validate_function_object(function: &Map<String, Value>, field: &str) -> Result<(), String> {
    for key in function.keys() {
        if !matches!(key.as_str(), "name" | "description" | "parameters" | "strict") {
            return Err(field_error(
                format!("{field}.{key}"),
                "is not supported by the model-pool contract",
            ));
        }
    }
    required_str(function.get("name"), &format!("{field}.name"))?;
    if let Some(strict) = function.get("strict").filter(|value| !value.is_null()) {
        if !strict.is_boolean() {
            return Err(field_error(&format!("{field}.strict"), "must be a boolean"));
        }
    }
    // `parameters` is an arbitrary JSON Schema and its contents are portable.
    Ok(())
}

fn validate_chat_tools(tools: &Value) -> Result<(), String> {
    let tools = tools
        .as_array()
        .ok_or_else(|| field_error("tools", "must be an array"))?;
    for tool in tools {
        let object = tool
            .as_object()
            .ok_or_else(|| field_error("tools", "every tool must be an object"))?;
        for key in object.keys() {
            if !matches!(key.as_str(), "type" | "function") {
                return Err(field_error(
                    format!("tools.{key}"),
                    "is not supported by the model-pool contract",
                ));
            }
        }
        let tool_type = required_str(object.get("type"), "tools.type")?;
        if tool_type != "function" {
            return Err(field_error(
                "tools",
                format!("hosted tool '{tool_type}' is not supported"),
            ));
        }
        let function = object
            .get("function")
            .and_then(Value::as_object)
            .ok_or_else(|| field_error("tools.function", "is required"))?;
        validate_function_object(function, "tools.function")?;
    }
    Ok(())
}

fn validate_chat_tool_choice(choice: &Value) -> Result<(), String> {
    if let Some(text) = choice.as_str() {
        return match text {
            "auto" | "none" | "required" => Ok(()),
            other => Err(field_error(
                "tool_choice",
                format!("unsupported choice '{other}'"),
            )),
        };
    }
    let object = choice
        .as_object()
        .ok_or_else(|| field_error("tool_choice", "must be a string or object"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "type" | "function") {
            return Err(field_error(
                format!("tool_choice.{key}"),
                "is not supported by the model-pool contract",
            ));
        }
    }
    match object.get("type").and_then(Value::as_str) {
        Some("function") => {
            let function = object
                .get("function")
                .and_then(Value::as_object)
                .ok_or_else(|| field_error("tool_choice.function", "is required"))?;
            for key in function.keys() {
                if key != "name" {
                    return Err(field_error(
                        format!("tool_choice.function.{key}"),
                        "is not supported by the model-pool contract",
                    ));
                }
            }
            required_str(function.get("name"), "tool_choice.function.name")?;
            Ok(())
        }
        Some("auto") | Some("none") | Some("required") => Ok(()),
        Some(other) => Err(field_error(
            "tool_choice",
            format!("unsupported choice '{other}'"),
        )),
        None => Err(field_error("tool_choice.type", "is required")),
    }
}

fn validate_responses(object: &Map<String, Value>) -> Result<(), String> {
    for field in ["previous_response_id", "conversation"] {
        if present_nonnull(object, field) {
            return Err(field_error(
                field,
                "is not supported for stateless model-pool requests",
            ));
        }
    }
    if let Some(n) = object.get("n") {
        if n.as_u64() != Some(1) {
            return Err(field_error(
                "n",
                "values other than 1 are unsupported by the model-pool contract",
            ));
        }
    }
    for field in ["truncation", "include"] {
        if present_nonnull(object, field) {
            return Err(field_error(
                field,
                "is not supported by the model-pool contract",
            ));
        }
    }
    if present_nonnull(object, "logprobs") {
        return Err(field_error(
            "logprobs",
            "is not supported by the model-pool contract",
        ));
    }
    if object.get("reasoning").is_some_and(nonempty) {
        return Err(field_error(
            "reasoning",
            "opaque reasoning state is not supported by the model-pool contract",
        ));
    }
    for field in object.keys() {
        if !RESPONSES_ALLOWED.contains(&field.as_str()) {
            return Err(field_error(
                field,
                "is not supported by the model-pool contract",
            ));
        }
    }
    for field in ["stream", "parallel_tool_calls"] {
        validate_bool(object, field)?;
    }
    for field in ["store", "background"] {
        validate_bool(object, field)?;
        if object.get(field).and_then(Value::as_bool) == Some(true) {
            return Err(field_error(
                field,
                "true is not supported by the model-pool contract",
            ));
        }
    }
    if let Some(text) = object.get("text").filter(|value| !value.is_null()) {
        validate_responses_text(text)?;
    }
    if let Some(tools) = object.get("tools").filter(|value| !value.is_null()) {
        validate_responses_tools(tools)?;
    }
    if let Some(choice) = object.get("tool_choice").filter(|value| !value.is_null()) {
        validate_responses_tool_choice(choice)?;
    }
    if let Some(input) = object.get("input") {
        validate_responses_input(input)?;
    }
    Ok(())
}

fn validate_responses_text(text: &Value) -> Result<(), String> {
    let object = text
        .as_object()
        .ok_or_else(|| field_error("text", "must be an object"))?;
    for key in object.keys() {
        if key != "format" {
            return Err(field_error(
                format!("text.{key}"),
                "is not supported by the model-pool contract",
            ));
        }
    }
    if let Some(format) = object.get("format").filter(|value| !value.is_null()) {
        let format = format
            .as_object()
            .ok_or_else(|| field_error("format", "must be an object"))?;
        for key in format.keys() {
            if key != "type" {
                return Err(field_error(
                    format!("format.{key}"),
                    "is not supported by the model-pool contract",
                ));
            }
        }
        if format.get("type").and_then(Value::as_str) != Some("text") {
            return Err(field_error(
                "format",
                "only text output is supported by the model-pool contract",
            ));
        }
    }
    Ok(())
}

fn validate_responses_tools(tools: &Value) -> Result<(), String> {
    let tools = tools
        .as_array()
        .ok_or_else(|| field_error("tools", "must be an array"))?;
    for tool in tools {
        let object = tool
            .as_object()
            .ok_or_else(|| field_error("tools", "every tool must be an object"))?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "type" | "name" | "description" | "parameters" | "strict"
            ) {
                return Err(field_error(
                    format!("tools.{key}"),
                    "is not supported by the model-pool contract",
                ));
            }
        }
        let tool_type = required_str(object.get("type"), "tools.type")?;
        if tool_type != "function" {
            return Err(field_error(
                "tools",
                format!("hosted tool '{tool_type}' is not supported"),
            ));
        }
        required_str(object.get("name"), "tools.name")?;
        if let Some(strict) = object.get("strict").filter(|value| !value.is_null()) {
            if !strict.is_boolean() {
                return Err(field_error("tools.strict", "must be a boolean"));
            }
        }
    }
    Ok(())
}

fn validate_responses_tool_choice(choice: &Value) -> Result<(), String> {
    if let Some(text) = choice.as_str() {
        return match text {
            "auto" | "none" | "required" => Ok(()),
            other => Err(field_error(
                "tool_choice",
                format!("unsupported choice '{other}'"),
            )),
        };
    }
    let object = choice
        .as_object()
        .ok_or_else(|| field_error("tool_choice", "must be a string or object"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "type" | "name") {
            return Err(field_error(
                format!("tool_choice.{key}"),
                "is not supported by the model-pool contract",
            ));
        }
    }
    match object.get("type").and_then(Value::as_str) {
        Some("function") => {
            required_str(object.get("name"), "tool_choice.name")?;
            Ok(())
        }
        Some("auto") | Some("none") | Some("required") => Ok(()),
        Some(other) => Err(field_error(
            "tool_choice",
            format!("unsupported choice '{other}'"),
        )),
        None => Err(field_error("tool_choice.type", "is required")),
    }
}

fn validate_responses_input(input: &Value) -> Result<(), String> {
    match input {
        Value::String(_) => Ok(()),
        Value::Array(items) => {
            for item in items {
                validate_responses_item(item)?;
            }
            Ok(())
        }
        Value::Object(_) => validate_responses_item(input),
        _ => Err(field_error("input", "must be a string or an array")),
    }
}

fn validate_responses_item(item: &Value) -> Result<(), String> {
    let object = item
        .as_object()
        .ok_or_else(|| field_error("input", "every item must be an object"))?;
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "type" | "role" | "content" | "call_id" | "id" | "name" | "arguments" | "output"
        ) {
            return Err(field_error(
                format!("input.{key}"),
                "is not supported by the model-pool contract",
            ));
        }
    }
    match object.get("type").and_then(Value::as_str) {
        Some("item_reference") => Err(field_error(
            "item_reference",
            "unresolved item references are not supported by the model-pool contract",
        )),
        Some("reasoning") => Err(field_error(
            "reasoning",
            "opaque reasoning state is not supported by the model-pool contract",
        )),
        Some("function_call") => {
            required_str(object.get("name"), "input.function_call.name")?;
            let arguments = required_str(
                object.get("arguments"),
                "input.function_call.arguments",
            )?;
            if serde_json::from_str::<Value>(arguments).is_err() {
                return Err(field_error(
                    "input.function_call.arguments",
                    "must be a JSON string",
                ));
            }
            Ok(())
        }
        Some("function_call_output") => {
            required_str(
                object.get("call_id"),
                "input.function_call_output.call_id",
            )?;
            if let Some(output) = object.get("output").filter(|value| !value.is_null()) {
                validate_responses_output(output)?;
            }
            Ok(())
        }
        Some("message") | None => {
            // `name` is admitted by the item union for a `function_call` item,
            // but a `message` item has no faithful Chat representation for it,
            // so it is rejected here rather than silently dropped.
            if present_nonnull(object, "name") {
                return Err(field_error(
                    "input.name",
                    "is not supported by the model-pool contract",
                ));
            }
            if let Some(content) = object.get("content") {
                validate_responses_content(content)?;
            }
            Ok(())
        }
        Some(other) => Err(field_error(
            other,
            "is not supported by the model-pool contract",
        )),
    }
}

fn validate_responses_output(output: &Value) -> Result<(), String> {
    match output {
        Value::String(_) => Ok(()),
        Value::Array(blocks) => {
            for block in blocks {
                let object = block
                    .as_object()
                    .ok_or_else(|| field_error("output", "invalid content block"))?;
                for key in object.keys() {
                    if !matches!(key.as_str(), "type" | "text" | "annotations") {
                        return Err(field_error(
                            format!("output.{key}"),
                            "is not supported by the model-pool contract",
                        ));
                    }
                }
                match object.get("type").and_then(Value::as_str) {
                    Some("input_text") | Some("output_text") | Some("text") => {
                        if object.get("text").and_then(Value::as_str).is_none() {
                            return Err(field_error("output.text", "expected a string"));
                        }
                    }
                    Some("input_image") | Some("image") => {
                        return Err(field_error(
                            "output",
                            "image output is not supported by the model-pool contract",
                        ))
                    }
                    Some("input_file") | Some("file") => {
                        return Err(field_error(
                            "output",
                            "file output is not supported by the model-pool contract",
                        ))
                    }
                    Some("input_audio") | Some("audio") => {
                        return Err(field_error(
                            "output",
                            "audio output is not supported by the model-pool contract",
                        ))
                    }
                    Some(other) => {
                        return Err(field_error(
                            "output",
                            format!("unsupported content block '{other}'"),
                        ))
                    }
                    None => return Err(field_error("output.type", "is required")),
                }
            }
            Ok(())
        }
        _ => Err(field_error("output", "must be text or content blocks")),
    }
}

fn validate_responses_content(content: &Value) -> Result<(), String> {
    match content {
        Value::String(_) => Ok(()),
        Value::Array(blocks) => {
            for block in blocks {
                let object = block
                    .as_object()
                    .ok_or_else(|| field_error("input.content", "invalid content block"))?;
                for key in object.keys() {
                    if !matches!(key.as_str(), "type" | "text" | "annotations") {
                        return Err(field_error(
                            format!("input.content.{key}"),
                            "is not supported by the model-pool contract",
                        ));
                    }
                }
                match object.get("type").and_then(Value::as_str) {
                    Some("input_text") | Some("output_text") | Some("text") => {
                        if object.get("text").and_then(Value::as_str).is_none() {
                            return Err(field_error("input.content.text", "expected a string"));
                        }
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
                            other,
                            "is not supported by the model-pool contract",
                        ))
                    }
                    None => return Err(field_error("input.content", "type is required")),
                }
            }
            Ok(())
        }
        _ => Err(field_error("input.content", "must be text or content blocks")),
    }
}

