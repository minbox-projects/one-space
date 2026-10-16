//! Read-only observation of the first meaningful upstream stream output and the
//! truthful whole-attempt output rate
//! (20261014-gateway-request-latency: REQ-002/REQ-003, AC-002/AC-003).
//!
//! The observer is fed the raw upstream bytes exactly as the relay receives
//! them -- before any protocol conversion -- together with the monotonic receipt
//! instant, and is finished with the monotonic end instant. It never influences
//! the forwarded bytes, the usage capture or the converters.
//!
//! First-token latency is the monotonic interval from the attempt start to the
//! first complete, valid, meaningful upstream event (non-empty text,
//! reasoning/thinking, a generated tool name or generated tool arguments). A
//! heartbeat/comment, an empty delta, a role-only or other metadata event, a
//! usage-only event, an error event or a completion marker is never output.
//! Average TPS is the explicitly reported valid cumulative output-token count
//! divided by the unrounded positive whole-attempt interval (the attempt start
//! to its end, including any pre-output wait); the interval is kept as a real
//! instant difference so neither the rounded `duration_ms` nor a clamped
//! denominator can leak into the rate. A first meaningful output is still
//! required to make the attempt eligible for a rate.

use super::UpstreamProtocol;
use serde_json::Value;
use tokio::time::Instant;

/// Bound on an unterminated SSE buffer, mirroring the usage accumulator: a
/// pathological stream that never emits a blank line cannot grow unbounded.
const MAX_BUFFER_BYTES: usize = 256 * 1024;

/// The first meaningful upstream output and the whole-attempt window of one
/// upstream attempt. One observer exists per attempt (retries and probes always
/// start from a fresh observation).
pub(in crate::ai_gateway) struct OutputTimingObserver {
    protocol: UpstreamProtocol,
    started_at: Instant,
    buffer: Vec<u8>,
    first_output_at: Option<Instant>,
    finished_at: Option<Instant>,
}

impl OutputTimingObserver {
    /// Start observing one attempt at its monotonic start.
    pub(in crate::ai_gateway) fn new(protocol: UpstreamProtocol, started_at: Instant) -> Self {
        Self {
            protocol,
            started_at,
            buffer: Vec::new(),
            first_output_at: None,
            finished_at: None,
        }
    }

    /// Feed one raw upstream chunk with its monotonic receipt instant. Complete
    /// SSE events (LF or CRLF, multiline data, possibly fragmented) are
    /// recognized incrementally; the receipt instant of the chunk that completes
    /// the first meaningful event becomes the first-output observation.
    pub(in crate::ai_gateway) fn feed_at(&mut self, chunk: &[u8], received_at: Instant) {
        self.buffer.extend_from_slice(chunk);
        while let Some(end) = event_end(&self.buffer) {
            let event: Vec<u8> = self.buffer.drain(..end).collect();
            self.observe_event(&event, received_at);
        }
        if self.buffer.len() > MAX_BUFFER_BYTES {
            self.buffer.clear();
        }
    }

    /// Finish the attempt at `ended_at`. A final meaningful event that arrived
    /// without the usual blank-line terminator is recognized here, and the end
    /// instant closes the whole-attempt window. The first call wins, so an end
    /// observed while forwarding is not overwritten by later bookkeeping.
    pub(in crate::ai_gateway) fn finish_at(&mut self, ended_at: Instant) {
        if self.finished_at.is_none() {
            self.finished_at = Some(ended_at);
        }
        if !self.buffer.is_empty() {
            let event = std::mem::take(&mut self.buffer);
            self.observe_event(&event, ended_at);
        }
    }

    /// Measured first-token latency in milliseconds, unrounded. `Some(0.0)` is a
    /// valid observation of an output at the attempt start; `None` means no
    /// meaningful output was ever observed.
    pub(in crate::ai_gateway) fn first_token_ms(&self) -> Option<f64> {
        let first = self.first_output_at?;
        Some(
            first
                .saturating_duration_since(self.started_at)
                .as_secs_f64()
                * 1000.0,
        )
    }

    /// Average output tokens per second over the unrounded whole-attempt window
    /// from the attempt start to its end (including any pre-output wait). A
    /// first meaningful output is still required to make the attempt eligible
    /// for a rate, and the first and end instants may coincide. `None` when no
    /// first output or no explicit valid count exists, or when the whole attempt
    /// is non-positive. An explicit zero count over a positive attempt is `0.0`.
    pub(in crate::ai_gateway) fn average_tps(
        &self,
        reported_output_tokens: Option<u64>,
    ) -> Option<f64> {
        // The first meaningful output stays required for eligibility; the
        // divisor is the whole attempt, so the pre-output wait counts too.
        if self.first_output_at.is_none() {
            return None;
        }
        let end = self.finished_at?;
        let seconds = end
            .saturating_duration_since(self.started_at)
            .as_secs_f64();
        if !(seconds > 0.0) {
            return None;
        }
        let tokens = reported_output_tokens?;
        Some(tokens as f64 / seconds)
    }

    /// Both persisted measurements for one attempt, derived from the same
    /// observed attempt-window instants and the independently validated reported
    /// output count.
    pub(in crate::ai_gateway) fn timing(
        &self,
        reported_output_tokens: Option<u64>,
    ) -> (Option<f64>, Option<f64>) {
        (
            self.first_token_ms(),
            self.average_tps(reported_output_tokens),
        )
    }

    fn observe_event(&mut self, bytes: &[u8], at: Instant) {
        if self.first_output_at.is_some() {
            return;
        }
        let Some(value) = parse_event_data(bytes) else {
            return;
        };
        if is_meaningful_output(self.protocol, &value) {
            self.first_output_at = Some(at);
        }
    }
}

/// The earliest SSE event boundary. A mixed-buffer stream must not merge an
/// earlier LF-delimited event into a later CRLF boundary.
fn event_end(buffer: &[u8]) -> Option<usize> {
    let lf = buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|index| index + 2);
    let crlf = buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4);
    match (lf, crlf) {
        (Some(lf), Some(crlf)) => Some(lf.min(crlf)),
        (Some(lf), None) => Some(lf),
        (None, Some(crlf)) => Some(crlf),
        (None, None) => None,
    }
}

/// Parse one complete SSE event into its JSON `data` payload. Multiline `data:`
/// lines are joined with `\n`; a comment, an empty payload, `[DONE]`, invalid
/// UTF-8 or malformed JSON yields `None`.
fn parse_event_data(bytes: &[u8]) -> Option<Value> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut data_lines: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.trim_start());
        }
    }
    if data_lines.is_empty() {
        return None;
    }
    let data = data_lines.join("\n");
    if data.is_empty() || data == "[DONE]" {
        return None;
    }
    serde_json::from_str(&data).ok()
}

/// Whether one parsed upstream event carries meaningful generated output for
/// its actual upstream protocol.
fn is_meaningful_output(protocol: UpstreamProtocol, value: &Value) -> bool {
    match protocol {
        UpstreamProtocol::ChatCompletions => chat_meaningful(value),
        UpstreamProtocol::Responses => responses_meaningful(value),
        UpstreamProtocol::AnthropicMessages => anthropic_meaningful(value),
    }
}

/// Chat Completions: an ordinary text delta, a reasoning/thinking delta, a
/// generated tool name/arguments, or the deprecated same-protocol
/// `delta.function_call` name/arguments. A role-only or empty delta, a
/// `usage`-only or `finish_reason`-only event and metadata envelopes carry no
/// output.
fn chat_meaningful(value: &Value) -> bool {
    let Some(choices) = value.get("choices").and_then(Value::as_array) else {
        return false;
    };
    for choice in choices {
        let Some(delta) = choice.get("delta").and_then(Value::as_object) else {
            continue;
        };
        for key in ["content", "reasoning_content", "reasoning"] {
            if non_empty_str(delta.get(key)) {
                return true;
            }
        }
        if let Some(function) = delta.get("function_call") {
            if non_empty_str(function.get("name")) || non_empty_str(function.get("arguments")) {
                return true;
            }
        }
        if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
            if calls.iter().any(tool_call_has_output) {
                return true;
            }
        }
    }
    false
}

/// A generated tool call carries output when its name or arguments are a
/// non-empty string (whitespace counts).
fn tool_call_has_output(call: &Value) -> bool {
    let function = call.get("function").unwrap_or(&Value::Null);
    non_empty_str(function.get("name")) || non_empty_str(function.get("arguments"))
}

/// Responses: an output-text, reasoning-summary or function-argument delta, or
/// the addition/completion of a function-call item that already carries a
/// non-empty name or arguments. `response.created`, lifecycle and
/// `response.completed` envelopes are not output.
fn responses_meaningful(value: &Value) -> bool {
    match value.get("type").and_then(Value::as_str).unwrap_or_default() {
        "response.output_text.delta"
        | "response.reasoning_summary_text.delta"
        | "response.reasoning_text.delta"
        | "response.function_call_arguments.delta" => non_empty_str(value.get("delta")),
        "response.output_item.added" | "response.output_item.done" => {
            let item = value.get("item").unwrap_or(&Value::Null);
            item.get("type").and_then(Value::as_str) == Some("function_call")
                && (non_empty_str(item.get("name")) || non_empty_str(item.get("arguments")))
        }
        _ => false,
    }
}

/// Anthropic Messages: a non-empty text/thinking/partial-json delta, a
/// `tool_use` content block that starts with a non-empty generated name, or a
/// `thinking` content block that starts with non-empty reasoning text.
/// `message_start`, `message_delta`, `message_stop`, `ping` and error events are
/// not output.
fn anthropic_meaningful(value: &Value) -> bool {
    match value.get("type").and_then(Value::as_str).unwrap_or_default() {
        "content_block_start" => {
            let block = value.get("content_block").unwrap_or(&Value::Null);
            match block.get("type").and_then(Value::as_str) {
                Some("tool_use") => non_empty_str(block.get("name")),
                Some("text") => non_empty_str(block.get("text")),
                Some("thinking") => non_empty_str(block.get("thinking")),
                _ => false,
            }
        }
        "content_block_delta" => {
            let delta = value.get("delta").unwrap_or(&Value::Null);
            match delta.get("type").and_then(Value::as_str) {
                Some("text_delta") => non_empty_str(delta.get("text")),
                Some("thinking_delta") => non_empty_str(delta.get("thinking")),
                Some("input_json_delta") => non_empty_str(delta.get("partial_json")),
                _ => false,
            }
        }
        _ => false,
    }
}

/// A present string that is not empty. Whitespace is output and is never
/// trimmed.
fn non_empty_str(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|text| !text.is_empty())
}
