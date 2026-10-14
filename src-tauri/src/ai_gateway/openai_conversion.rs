//! Pool-only Chat Completions <-> Responses request/response conversion and the
//! portable request contract (20261013-free-model-pool: REQ-004/REQ-005/REQ-007).
//!
//! The ordinary gateway forwards an OpenAI request to the candidate's own
//! endpoint and converts only OpenAI -> native Anthropic Messages. A model pool
//! may select a candidate whose stored endpoint differs from the client's, so
//! this module supplies the missing OpenAI <-> OpenAI translation together with
//! the portable-field contract that must be enforced *before* any candidate is
//! selected.
//!
//! This module never touches ordinary routing, key policy, health settlement or
//! Protocol Router; it only shapes pool request and response bodies. The
//! session-affinity layer, scheduler and streaming lifecycle stay in
//! `runtime_http.rs`.

use super::UpstreamProtocol;
use serde_json::{json, Map, Value};

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

fn field_error(field: impl AsRef<str>, message: impl Into<String>) -> String {
    format!("field '{}': {}", field.as_ref(), message.into())
}

fn nonempty(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(_) => true,
        Value::String(text) => !text.trim().is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(object) => !object.is_empty(),
    }
}

fn present_nonnull(object: &Map<String, Value>, field: &str) -> bool {
    object.get(field).is_some_and(|value| !value.is_null())
}

/// A required, non-blank string field.
fn required_str<'a>(value: Option<&'a Value>, field: &str) -> Result<&'a str, String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| field_error(field, "is required and must be a non-empty string"))
}

/// Validate a boolean-valued field, rejecting any non-boolean supplied value.
fn validate_bool(object: &Map<String, Value>, field: &str) -> Result<(), String> {
    if let Some(value) = object.get(field).filter(|value| !value.is_null()) {
        if !value.is_boolean() {
            return Err(field_error(field, "must be a boolean"));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Portable-field contract (REQ-005 / AC-007)
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
        super::anthropic::validate_conversion_request(path, body)?;
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
