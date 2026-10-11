//! Read-only observation of native Anthropic Messages SSE streams.

use super::usage_log::{canonical_usage_from_value, CanonicalUsage};
use serde_json::Value;

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
