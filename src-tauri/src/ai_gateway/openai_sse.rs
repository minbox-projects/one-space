//! Pool-only incremental Chat Completions <-> Responses SSE conversion
//! (20261013-free-model-pool: REQ-004/REQ-005/REQ-007; AC-006/AC-017).
//!
//! The converter consumes the real upstream stream and emits the client's own
//! protocol incrementally: it buffers only until an SSE event boundary, never
//! the whole answer, so a converted delta reaches the caller before the upstream
//! completes. It rewrites the public pool alias in both directions, preserves
//! stable message/tool identities, distinguishes a function-call item id from
//! its call id, emits the canonical Responses lifecycle, and reports whether the
//! upstream stream actually completed so a malformed, in-band-errored or
//! premature stream is never mistaken for a success.
//!
//! It is used only for prepared model-pool candidates on an OpenAI endpoint;
//! native Anthropic Messages streams keep their existing adapter.

use super::UpstreamProtocol;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// The earliest SSE event boundary. A mixed-buffer stream must not merge an
/// earlier LF-delimited event into a later CRLF boundary.
fn event_end(buffer: &[u8]) -> Option<usize> {
    let lf = buffer.windows(2).position(|window| window == b"\n\n").map(|i| i + 2);
    let crlf = buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|i| i + 4);
    match (lf, crlf) {
        (Some(lf), Some(crlf)) => Some(lf.min(crlf)),
        (Some(lf), None) => Some(lf),
        (None, Some(crlf)) => Some(crlf),
        (None, None) => None,
    }
}

struct SseEvent<'a> {
    name: Option<&'a str>,
    data: Option<String>,
}

fn parse_event(text: &str) -> SseEvent<'_> {
    let mut name = None;
    let mut data: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(value) = line.strip_prefix("event:") {
            name = Some(value.trim());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.trim_start());
        }
    }
    SseEvent {
        name,
        data: (!data.is_empty()).then(|| data.join("\n")),
    }
}

fn chat_event(value: Value) -> Vec<u8> {
    format!("data: {value}\n\n").into_bytes()
}

fn responses_event(kind: &str, value: Value) -> Vec<u8> {
    format!("event: {kind}\ndata: {value}\n\n").into_bytes()
}

fn chat_chunk(id: &str, model: &str, delta: Value, finish_reason: Value) -> Value {
    json!({
        "id": id,
        "object": "chat.completion.chunk",
        "model": model,
        "choices": [{"index": 0, "delta": delta, "finish_reason": finish_reason}],
    })
}

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

/// Whether a Responses event denotes a source failure rather than a completed
/// response. Covers the explicit error/failed/incomplete types and a
/// `response.completed` whose response status is not `completed`.
fn responses_failure(value: &Value) -> bool {
    let kind = value.get("type").and_then(Value::as_str).unwrap_or_default();
    if matches!(kind, "error" | "response.failed" | "response.incomplete") {
        return true;
    }
    if kind == "response.completed" {
        if let Some(status) = value
            .get("response")
            .and_then(|response| response.get("status"))
            .and_then(Value::as_str)
        {
            if status != "completed" {
                return true;
            }
        }
    }
    false
}

/// One Chat -> Responses output slot, retained in arrival order.
struct Output {
    message: bool,
    index: usize,
    item_id: String,
    name: String,
    text: String,
    arguments: String,
    item_added: bool,
    text_done: bool,
    part_done: bool,
    args_done: bool,
    item_done: bool,
}

impl Output {
    fn message(index: usize, item_id: String) -> Self {
        Self {
            message: true,
            index,
            item_id,
            name: String::new(),
            text: String::new(),
            arguments: String::new(),
            item_added: false,
            text_done: false,
            part_done: false,
            args_done: false,
            item_done: false,
        }
    }

    fn function_call(index: usize, item_id: String, name: String) -> Self {
        Self {
            message: false,
            index,
            item_id,
            name,
            text: String::new(),
            arguments: String::new(),
            item_added: true,
            text_done: false,
            part_done: false,
            args_done: false,
            item_done: false,
        }
    }

    fn completed_item(&self) -> Value {
        if self.message {
            json!({
                "type": "message",
                "id": self.item_id,
                "role": "assistant",
                "status": "completed",
                "content": if self.text.is_empty() {
                    json!([])
                } else {
                    json!([{"type": "output_text", "text": self.text, "annotations": []}])
                },
            })
        } else {
            json!({
                "type": "function_call",
                "id": self.item_id,
                "call_id": self.item_id,
                "name": self.name,
                "arguments": self.arguments,
                "status": "completed",
            })
        }
    }
}

pub(in crate::ai_gateway) struct OpenAiSseConverter {
    buffer: Vec<u8>,
    client: UpstreamProtocol,
    upstream: UpstreamProtocol,
    model: String,
    include_usage: bool,
    /// Responses upstream -> Chat client: tool item id -> stable Chat index.
    tool_index_by_item: HashMap<String, usize>,
    /// Responses upstream -> Chat client: stable Chat index -> call id / name.
    tool_call_ids: HashMap<usize, String>,
    tool_names: HashMap<usize, String>,
    tool_count: usize,
    /// Chat upstream -> Responses client: the upstream chat id and outputs.
    response_id: String,
    created_sent: bool,
    outputs: Vec<Output>,
    tool_output_by_chat_index: HashMap<usize, usize>,
    /// Last usage object observed upstream (either shape).
    usage: Value,
    completed: bool,
    failed: bool,
}

impl OpenAiSseConverter {
    pub(in crate::ai_gateway) fn new(
        client_path: &str,
        upstream: UpstreamProtocol,
        model: &str,
        include_usage: bool,
    ) -> Self {
        let client = super::openai_conversion::client_protocol(client_path)
            .unwrap_or(UpstreamProtocol::ChatCompletions);
        Self {
            buffer: Vec::new(),
            client,
            upstream,
            model: model.to_string(),
            include_usage,
            tool_index_by_item: HashMap::new(),
            tool_call_ids: HashMap::new(),
            tool_names: HashMap::new(),
            tool_count: 0,
            response_id: String::new(),
            created_sent: false,
            outputs: Vec::new(),
            tool_output_by_chat_index: HashMap::new(),
            usage: Value::Object(Map::new()),
            completed: false,
            failed: false,
        }
    }

    pub(in crate::ai_gateway) fn is_success(&self) -> bool {
        self.completed && !self.failed
    }

    pub(in crate::ai_gateway) fn has_in_band_error(&self) -> bool {
        self.failed
    }

    pub(in crate::ai_gateway) fn feed(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        self.buffer.extend_from_slice(chunk);
        let mut output = Vec::new();
        while let Some(end) = event_end(&self.buffer) {
            let event = self.buffer.drain(..end).collect::<Vec<_>>();
            output.extend(self.convert_event(&event)?);
        }
        Ok(output)
    }

    pub(in crate::ai_gateway) fn finish(&mut self) -> Result<Vec<Vec<u8>>, String> {
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }
        let event = std::mem::take(&mut self.buffer);
        self.convert_event(&event)
    }

    fn convert_event(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        if self.failed {
            return Ok(Vec::new());
        }
        // A frame that is not valid UTF-8 is a malformed stream, never a frame
        // to silently drop.
        let Ok(text) = std::str::from_utf8(bytes) else {
            self.failed = true;
            return Ok(Vec::new());
        };
        let event = parse_event(text);
        let Some(data) = event.data.as_deref() else {
            return Ok(Vec::new());
        };
        if data.is_empty() {
            return Ok(Vec::new());
        }
        if self.upstream == self.client {
            Ok(self.alias_event(event.name, data))
        } else {
            match (self.upstream, self.client) {
                (UpstreamProtocol::Responses, UpstreamProtocol::ChatCompletions) => {
                    Ok(self.responses_to_chat_event(event.name, data))
                }
                (UpstreamProtocol::ChatCompletions, UpstreamProtocol::Responses) => {
                    Ok(self.chat_to_responses_event(data))
                }
                _ => Ok(Vec::new()),
            }
        }
    }

    /// Same-protocol pass-through that validates every frame, detects a source
    /// error, rewrites the public pool alias and only marks a genuine completion.
    fn alias_event(&mut self, event_name: Option<&str>, data: &str) -> Vec<Vec<u8>> {
        if data == "[DONE]" {
            if self.upstream == UpstreamProtocol::ChatCompletions {
                self.completed = true;
                return vec![b"data: [DONE]\n\n".to_vec()];
            }
            return Vec::new();
        }
        let Ok(mut value) = serde_json::from_str::<Value>(data) else {
            self.failed = true;
            return Vec::new();
        };
        let is_responses = self.upstream == UpstreamProtocol::Responses;
        if is_responses {
            if responses_failure(&value) {
                self.failed = true;
                return Vec::new();
            }
        } else if value.get("error").is_some_and(Value::is_object) {
            self.failed = true;
            return Vec::new();
        }
        let Some(object) = value.as_object_mut() else {
            self.failed = true;
            return Vec::new();
        };
        object.insert("model".to_string(), Value::String(self.model.clone()));
        if let Some(response) = object.get_mut("response").and_then(Value::as_object_mut) {
            response.insert("model".to_string(), Value::String(self.model.clone()));
        }
        if object.get("type").and_then(Value::as_str) == Some("response.completed") {
            self.completed = true;
        }
        if is_responses {
            let kind = object
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| event_name.map(str::to_string))
                .unwrap_or_else(|| "message".to_string());
            vec![responses_event(&kind, value)]
        } else {
            vec![chat_event(value)]
        }
    }

    /// Responses upstream -> Chat client.
    fn responses_to_chat_event(
        &mut self,
        event_name: Option<&str>,
        data: &str,
    ) -> Vec<Vec<u8>> {
        if data == "[DONE]" {
            return Vec::new();
        }
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            self.failed = true;
            return Vec::new();
        };
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .or(event_name)
            .unwrap_or_default();
        if responses_failure(&value) {
            self.failed = true;
            return Vec::new();
        }
        match kind {
            "response.created" => {
                if let Some(id) = value
                    .get("response")
                    .and_then(|response| response.get("id"))
                    .and_then(Value::as_str)
                {
                    self.response_id = id.to_string();
                }
                Vec::new()
            }
            "response.output_item.added" => {
                let item = value.get("item").unwrap_or(&Value::Null);
                if item.get("type").and_then(Value::as_str) != Some("function_call") {
                    return Vec::new();
                }
                // The stable client id is the call id; the delta frames key by
                // the function-call item id. They are not interchangeable.
                let item_id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let call_id = item
                    .get("call_id")
                    .and_then(Value::as_str)
                    .filter(|id| !id.is_empty())
                    .unwrap_or(item_id.as_str())
                    .to_string();
                let name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let index = self.tool_count;
                self.tool_count += 1;
                self.tool_index_by_item.insert(item_id, index);
                self.tool_call_ids.insert(index, call_id.clone());
                self.tool_names.insert(index, name.clone());
                vec![chat_event(chat_chunk(
                    &self.chunk_id(),
                    &self.model,
                    json!({"tool_calls": [{"index": index, "id": call_id, "type": "function", "function": {"name": name, "arguments": ""}}]}),
                    Value::Null,
                ))]
            }
            "response.function_call_arguments.delta" => {
                let item_id = value
                    .get("item_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let Some(index) = self.tool_index_by_item.get(item_id).copied() else {
                    return Vec::new();
                };
                let delta = value
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if delta.is_empty() {
                    return Vec::new();
                }
                let call_id = self.tool_call_ids.get(&index).cloned().unwrap_or_default();
                let name = self.tool_names.get(&index).cloned().unwrap_or_default();
                vec![chat_event(chat_chunk(
                    &self.chunk_id(),
                    &self.model,
                    json!({"tool_calls": [{"index": index, "id": call_id, "type": "function", "function": {"name": name, "arguments": delta}}]}),
                    Value::Null,
                ))]
            }
            "response.output_text.delta" => {
                let delta = value
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if delta.is_empty() {
                    return Vec::new();
                }
                vec![chat_event(chat_chunk(
                    &self.chunk_id(),
                    &self.model,
                    json!({"content": delta}),
                    Value::Null,
                ))]
            }
            "response.completed" => {
                if let Some(usage) = value
                    .get("response")
                    .and_then(|response| response.get("usage"))
                    .filter(|usage| usage.is_object())
                {
                    self.usage = usage.clone();
                }
                self.completed = true;
                let finish_reason = if self.tool_count > 0 {
                    "tool_calls"
                } else {
                    "stop"
                };
                let mut events = vec![chat_event(chat_chunk(
                    &self.chunk_id(),
                    &self.model,
                    json!({}),
                    Value::String(finish_reason.to_string()),
                ))];
                if self.include_usage
                    && self.usage.is_object()
                    && !self.usage.as_object().is_some_and(|usage| usage.is_empty())
                {
                    let mut usage = json!({
                        "prompt_tokens": usage_input(&self.usage),
                        "completion_tokens": usage_output(&self.usage),
                        "total_tokens": usage_total(&self.usage),
                    });
                    if let Some(cached) = cached_tokens(&self.usage) {
                        usage["prompt_tokens_details"] = json!({"cached_tokens": cached});
                    }
                    let mut chunk = chat_chunk(&self.chunk_id(), &self.model, json!({}), Value::Null);
                    chunk["choices"] = json!([]);
                    chunk["usage"] = usage;
                    events.push(chat_event(chunk));
                }
                events.push(b"data: [DONE]\n\n".to_vec());
                events
            }
            _ => Vec::new(),
        }
    }

    /// Chat upstream -> Responses client.
    fn chat_to_responses_event(&mut self, data: &str) -> Vec<Vec<u8>> {
        if data == "[DONE]" {
            self.completed = true;
            return self.finish_completion();
        }
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            self.failed = true;
            return Vec::new();
        };
        let Some(object) = value.as_object() else {
            self.failed = true;
            return Vec::new();
        };
        // A root error object is a semantic failure even when it precedes any
        // output; it must never be followed by a fabricated completion.
        if object.get("error").is_some_and(Value::is_object) {
            self.failed = true;
            return Vec::new();
        }
        if self.response_id.is_empty() {
            if let Some(id) = object.get("id").and_then(Value::as_str) {
                self.response_id = id.to_string();
            }
        }
        if let Some(usage) = object.get("usage").filter(|usage| usage.is_object()) {
            self.usage = usage.clone();
        }
        let mut events = Vec::new();
        if let Some(delta) = object
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("delta"))
            .and_then(Value::as_object)
        {
            if let Some(content) = delta.get("content").and_then(Value::as_str) {
                if !content.is_empty() {
                    events.extend(self.append_text(content));
                }
            }
            if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
                for call in calls {
                    events.extend(self.append_tool_delta(call));
                }
            }
        }
        events
    }

    fn ensure_created(&mut self) -> Vec<Vec<u8>> {
        if self.created_sent {
            return Vec::new();
        }
        self.created_sent = true;
        if self.response_id.is_empty() {
            self.response_id = "resp_pool".to_string();
        }
        vec![responses_event(
            "response.created",
            json!({
                "type": "response.created",
                "response": {"id": self.response_id, "object": "response", "model": self.model, "status": "in_progress", "output": []},
            }),
        )]
    }

    fn append_text(&mut self, text: &str) -> Vec<Vec<u8>> {
        let mut events = self.ensure_created();
        let index = match self.outputs.iter().position(|output| output.message) {
            Some(index) => index,
            None => {
                let index = self.outputs.len();
                let item_id = format!("{}_msg", self.chunk_id());
                self.outputs.push(Output::message(index, item_id));
                index
            }
        };
        {
            let output = &mut self.outputs[index];
            if !output.item_added {
                output.item_added = true;
                events.push(responses_event(
                    "response.output_item.added",
                    json!({
                        "type": "response.output_item.added",
                        "output_index": output.index,
                        "item": {"type": "message", "id": output.item_id, "role": "assistant", "status": "in_progress", "content": []},
                    }),
                ));
                events.push(responses_event(
                    "response.content_part.added",
                    json!({
                        "type": "response.content_part.added",
                        "item_id": output.item_id,
                        "output_index": output.index,
                        "content_index": 0,
                        "part": {"type": "output_text", "text": "", "annotations": []},
                    }),
                ));
            }
            output.text.push_str(text);
        }
        let output = &self.outputs[index];
        events.push(responses_event(
            "response.output_text.delta",
            json!({
                "type": "response.output_text.delta",
                "item_id": output.item_id,
                "output_index": output.index,
                "content_index": 0,
                "delta": text,
            }),
        ));
        events
    }

    fn append_tool_delta(&mut self, call: &Value) -> Vec<Vec<u8>> {
        let mut events = self.ensure_created();
        let chat_index = call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
        let function = call.get("function").unwrap_or(&Value::Null);
        if !self.tool_output_by_chat_index.contains_key(&chat_index) {
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let name = function
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if id.is_empty() || name.is_empty() {
                // A function-call item needs a stable id and name; without them
                // the conversion cannot be represented.
                self.failed = true;
                return events;
            }
            let index = self.outputs.len();
            self.outputs
                .push(Output::function_call(index, id.clone(), name.clone()));
            self.tool_output_by_chat_index.insert(chat_index, index);
            events.push(responses_event(
                "response.output_item.added",
                json!({
                    "type": "response.output_item.added",
                    "output_index": index,
                    "item": {"type": "function_call", "id": id, "call_id": id, "name": name, "arguments": "", "status": "in_progress"},
                }),
            ));
        }
        let output_index = self.tool_output_by_chat_index[&chat_index];
        if let Some(arguments) = function.get("arguments").and_then(Value::as_str) {
            if !arguments.is_empty() {
                let output = &mut self.outputs[output_index];
                output.arguments.push_str(arguments);
                events.push(responses_event(
                    "response.function_call_arguments.delta",
                    json!({
                        "type": "response.function_call_arguments.delta",
                        "item_id": output.item_id,
                        "output_index": output.index,
                        "delta": arguments,
                    }),
                ));
            }
        }
        events
    }

    fn finish_completion(&mut self) -> Vec<Vec<u8>> {
        let mut events = Vec::new();
        for index in 0..self.outputs.len() {
            let (message, output_index, item_id, text, arguments, item_added) = {
                let output = &self.outputs[index];
                (
                    output.message,
                    output.index,
                    output.item_id.clone(),
                    output.text.clone(),
                    output.arguments.clone(),
                    output.item_added,
                )
            };
            if !item_added {
                continue;
            }
            if message {
                if !self.outputs[index].text_done {
                    self.outputs[index].text_done = true;
                    events.push(responses_event(
                        "response.output_text.done",
                        json!({
                            "type": "response.output_text.done",
                            "item_id": item_id,
                            "output_index": output_index,
                            "content_index": 0,
                            "text": text,
                        }),
                    ));
                }
                if !self.outputs[index].part_done {
                    self.outputs[index].part_done = true;
                    events.push(responses_event(
                        "response.content_part.done",
                        json!({
                            "type": "response.content_part.done",
                            "item_id": item_id,
                            "output_index": output_index,
                            "content_index": 0,
                            "part": {"type": "output_text", "text": text, "annotations": []},
                        }),
                    ));
                }
            } else {
                if !self.outputs[index].args_done {
                    self.outputs[index].args_done = true;
                    events.push(responses_event(
                        "response.function_call_arguments.done",
                        json!({
                            "type": "response.function_call_arguments.done",
                            "item_id": item_id,
                            "output_index": output_index,
                            "arguments": arguments,
                        }),
                    ));
                }
            }
            if !self.outputs[index].item_done {
                self.outputs[index].item_done = true;
                let item = self.outputs[index].completed_item();
                events.push(responses_event(
                    "response.output_item.done",
                    json!({"type": "response.output_item.done", "output_index": output_index, "item": item}),
                ));
            }
        }
        let output: Vec<Value> = self
            .outputs
            .iter()
            .filter(|output| output.item_added)
            .map(Output::completed_item)
            .collect();
        events.push(responses_event(
            "response.completed",
            json!({
                "type": "response.completed",
                "response": {
                    "id": self.chunk_id(),
                    "object": "response",
                    "model": self.model,
                    "status": "completed",
                    "output": output,
                    "usage": {
                        "input_tokens": usage_input(&self.usage),
                        "output_tokens": usage_output(&self.usage),
                        "total_tokens": usage_total(&self.usage),
                    },
                },
            }),
        ));
        events
    }

    fn chunk_id(&self) -> String {
        if self.response_id.is_empty() {
            "resp_pool".to_string()
        } else {
            self.response_id.clone()
        }
    }
}
