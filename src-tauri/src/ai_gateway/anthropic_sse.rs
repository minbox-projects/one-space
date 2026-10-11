//! Read-only observation of native Anthropic Messages SSE streams.

use super::usage_log::{canonical_usage_from_value, CanonicalUsage};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Default)]
pub(in crate::ai_gateway) struct AnthropicSseObserver {
    buffer: Vec<u8>,
    usage: Option<Value>,
    message_stop: bool,
    in_band_error: bool,
}

impl AnthropicSseObserver {
    pub(in crate::ai_gateway) fn feed(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
        while let Some(end) = event_end(&self.buffer) {
            let event = self.buffer.drain(..end).collect::<Vec<_>>();
            self.observe_event(&event);
        }
    }

    pub(in crate::ai_gateway) fn finish(&mut self) {
        if !self.buffer.is_empty() {
            let event = std::mem::take(&mut self.buffer);
            self.observe_event(&event);
        }
    }

    pub(in crate::ai_gateway) fn is_success(&self) -> bool {
        self.message_stop && !self.in_band_error
    }

    pub(in crate::ai_gateway) fn has_in_band_error(&self) -> bool {
        self.in_band_error
    }

    pub(in crate::ai_gateway) fn canonical_usage(&self) -> Option<CanonicalUsage> {
        self.usage
            .as_ref()
            .filter(|usage| usage.is_object())
            .map(canonical_usage_from_value)
    }

    fn observe_event(&mut self, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        let mut event_name = None;
        let mut data_lines = Vec::new();
        for line in text.split('\n') {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if let Some(value) = line.strip_prefix("event:") {
                event_name = Some(value.trim());
            } else if let Some(value) = line.strip_prefix("data:") {
                data_lines.push(value.trim_start());
            }
        }

        let data = data_lines.join("\n");
        if data.is_empty() || data == "[DONE]" {
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(&data) else {
            return;
        };
        let kind = value.get("type").and_then(Value::as_str);
        if event_name == Some("message_stop") || kind == Some("message_stop") {
            self.message_stop = true;
        }
        if event_name == Some("error") || kind == Some("error") {
            self.in_band_error = true;
        }

        let usage = match kind {
            Some("message_start") => value
                .get("message")
                .and_then(|message| message.get("usage")),
            Some("message_delta") => value.get("usage"),
            _ => None,
        };
        if let Some(usage) = usage.filter(|usage| usage.is_object()) {
            self.merge_usage(usage);
        }
    }

    fn merge_usage(&mut self, usage: &Value) {
        let current = self
            .usage
            .get_or_insert_with(|| Value::Object(serde_json::Map::new()));
        let Some(current) = current.as_object_mut() else {
            *current = Value::Object(serde_json::Map::new());
            return self.merge_usage(usage);
        };
        let Some(incoming) = usage.as_object() else {
            return;
        };
        for (key, value) in incoming {
            if !value.is_null() {
                current.insert(key.clone(), value.clone());
            }
        }
    }
}

fn event_end(buffer: &[u8]) -> Option<usize> {
    if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
        return Some(index + 4);
    }
    buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|index| index + 2)
}

/// Incremental Anthropic-to-OpenAI SSE adapter. It deliberately emits only
/// caller-format events; pings and lifecycle events that have no caller-visible
/// content do not fabricate deltas.
pub(in crate::ai_gateway) struct AnthropicSseConverter {
    buffer: Vec<u8>,
    path: String,
    model: String,
    message_id: String,
    tool_ids: HashMap<usize, String>,
    tool_names: HashMap<usize, String>,
    usage: Value,
    failed: bool,
    stopped: bool,
}

impl AnthropicSseConverter {
    pub(in crate::ai_gateway) fn new(path: &str, model: &str) -> Self {
        Self {
            buffer: Vec::new(),
            path: path.to_string(),
            model: model.to_string(),
            message_id: String::new(),
            tool_ids: HashMap::new(),
            tool_names: HashMap::new(),
            usage: Value::Object(serde_json::Map::new()),
            failed: false,
            stopped: false,
        }
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

    pub(in crate::ai_gateway) fn stopped(&self) -> bool {
        self.stopped
    }

    fn is_responses(&self) -> bool {
        self.path.split('?').next().unwrap_or(&self.path) == "/v1/responses"
    }

    fn chat_event(value: Value) -> Vec<u8> {
        format!("data: {value}\n\n").into_bytes()
    }

    fn responses_event(kind: &str, value: Value) -> Vec<u8> {
        format!("event: {kind}\ndata: {value}\n\n").into_bytes()
    }

    fn convert_event(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        let text = String::from_utf8_lossy(bytes);
        let mut event_name = None;
        let mut data = Vec::new();
        for line in text.split('\n') {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if let Some(value) = line.strip_prefix("event:") {
                event_name = Some(value.trim());
            } else if let Some(value) = line.strip_prefix("data:") {
                data.push(value.trim_start());
            }
        }
        let data = data.join("\n");
        if data.is_empty() || data == "[DONE]" {
            return Ok(Vec::new());
        }
        let value: Value = serde_json::from_str(&data).map_err(|error| error.to_string())?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .or(event_name)
            .unwrap_or_default();
        match kind {
            "message_start" => {
                self.message_id = value
                    .get("message")
                    .and_then(|message| message.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if let Some(usage) = value
                    .get("message")
                    .and_then(|message| message.get("usage"))
                    .filter(|usage| usage.is_object())
                {
                    self.merge_usage(usage);
                }
                Ok(Vec::new())
            }
            "content_block_start" => {
                let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let block = value.get("content_block").unwrap_or(&Value::Null);
                if block.get("type").and_then(Value::as_str) != Some("tool_use") {
                    return Ok(Vec::new());
                }
                let id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.tool_ids.insert(index, id.clone());
                self.tool_names.insert(index, name.clone());
                if self.is_responses() {
                    Ok(vec![Self::responses_event(
                        "response.output_item.added",
                        serde_json::json!({"type":"response.output_item.added","item":{"type":"function_call","id":id,"call_id":id,"name":name,"arguments":""}}),
                    )])
                } else {
                    Ok(vec![Self::chat_event(serde_json::json!({
                        "id": self.message_id,
                        "object": "chat.completion.chunk",
                        "model": self.model,
                        "choices": [{"index":0,"delta":{"tool_calls":[{"index":index,"id":id,"type":"function","function":{"name":name,"arguments":""}}]},"finish_reason":null}],
                    }))])
                }
            }
            "content_block_delta" => {
                let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let delta = value.get("delta").unwrap_or(&Value::Null);
                match delta.get("type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        let text = delta
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        if text.is_empty() {
                            return Ok(Vec::new());
                        }
                        if self.is_responses() {
                            Ok(vec![Self::responses_event(
                                "response.output_text.delta",
                                serde_json::json!({"type":"response.output_text.delta","delta":text,"item_id":self.message_id}),
                            )])
                        } else {
                            Ok(vec![Self::chat_event(
                                serde_json::json!({"id":self.message_id,"object":"chat.completion.chunk","model":self.model,"choices":[{"index":0,"delta":{"content":text},"finish_reason":null}]}),
                            )])
                        }
                    }
                    Some("input_json_delta") => {
                        let partial = delta
                            .get("partial_json")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        if partial.is_empty() {
                            return Ok(Vec::new());
                        }
                        if self.is_responses() {
                            Ok(vec![Self::responses_event(
                                "response.function_call_arguments.delta",
                                serde_json::json!({"type":"response.function_call_arguments.delta","item_id":self.tool_ids.get(&index).cloned().unwrap_or_default(),"delta":partial}),
                            )])
                        } else {
                            Ok(vec![Self::chat_event(
                                serde_json::json!({"id":self.message_id,"object":"chat.completion.chunk","model":self.model,"choices":[{"index":0,"delta":{"tool_calls":[{"index":index,"id":self.tool_ids.get(&index).cloned().unwrap_or_default(),"type":"function","function":{"name":self.tool_names.get(&index).cloned().unwrap_or_default(),"arguments":partial}}]},"finish_reason":null}]}),
                            )])
                        }
                    }
                    _ => Ok(Vec::new()),
                }
            }
            "message_delta" => {
                if let Some(usage) = value.get("usage").filter(|usage| usage.is_object()) {
                    self.merge_usage(usage);
                }
                let stop = value
                    .get("delta")
                    .and_then(|delta| delta.get("stop_reason"))
                    .and_then(Value::as_str);
                if self.is_responses() || stop.is_none() {
                    return Ok(Vec::new());
                }
                let reason = if stop == Some("tool_use") {
                    "tool_calls"
                } else {
                    "stop"
                };
                let mut chunk = serde_json::json!({"id":self.message_id,"object":"chat.completion.chunk","model":self.model,"choices":[{"index":0,"delta":{},"finish_reason":reason}]});
                if !self.usage.as_object().is_some_and(|usage| usage.is_empty()) {
                    let canonical =
                        super::usage_log::canonical_usage_from_value(&self.usage).tokens;
                    let input = canonical
                        .input_tokens
                        .saturating_add(canonical.cache_read_tokens)
                        .saturating_add(canonical.cache_write_tokens);
                    chunk["usage"] = serde_json::json!({
                        "prompt_tokens": input,
                        "completion_tokens": canonical.output_tokens,
                        "total_tokens": input.saturating_add(canonical.output_tokens),
                        "prompt_tokens_details": {"cached_tokens": canonical.cache_read_tokens},
                    });
                }
                Ok(vec![Self::chat_event(chunk)])
            }
            "message_stop" => {
                self.stopped = true;
                if self.is_responses() {
                    Ok(vec![Self::responses_event(
                        "response.completed",
                        serde_json::json!({"type":"response.completed","response":{"id":self.message_id,"object":"response","model":self.model,"status":"completed"}}),
                    )])
                } else {
                    Ok(vec![b"data: [DONE]\n\n".to_vec()])
                }
            }
            "error" => {
                self.failed = true;
                let message = value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("upstream Anthropic error");
                if self.is_responses() {
                    Ok(vec![Self::responses_event(
                        "error",
                        serde_json::json!({"type":"error","error":{"message":message}}),
                    )])
                } else {
                    Ok(vec![Self::chat_event(
                        serde_json::json!({"error":{"message":message,"type":"server_error","code":"upstream_error"}}),
                    )])
                }
            }
            _ => Ok(Vec::new()),
        }
    }

    fn merge_usage(&mut self, usage: &Value) {
        let Some(incoming) = usage.as_object() else {
            return;
        };
        let Some(current) = self.usage.as_object_mut() else {
            return;
        };
        for (key, value) in incoming {
            if !value.is_null() {
                current.insert(key.clone(), value.clone());
            }
        }
    }
}
