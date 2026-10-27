//! Canonical token normalization and streaming usage accumulation.

use super::records::UsageTokens;
use serde_json::Value;

/// Canonical normalization result: mutually exclusive tiers plus whether the
/// source shape was a valid decomposition.
///
/// Invalid shapes (nested cache read conflicting with top-level
/// `cache_read_input_tokens`, or nested cache components exceeding the
/// reported inclusive input) fall back to the reported input with no cache
/// tiers so billing stays conservative (no double counting, no negative)
/// while the store layer keeps the row cache-statistics-ineligible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ai_gateway) struct CanonicalUsage {
    pub tokens: UsageTokens,
    pub valid: bool,
    /// Independently validated explicitly reported cumulative output count.
    /// `Some` only for a
    /// non-negative integer `output_tokens` (falling back to `completion_tokens`)
    /// or its supported numeric-string form; missing, null, negative,
    /// fractional or malformed values stay `None`. This is derived directly from
    /// the selected raw field, so it is independent of the input/cache
    /// decomposition validity and never changes billing tokens or costs.
    pub reported_output_tokens: Option<u64>,
}

fn token_number(value: Option<&Value>) -> u64 {
    match value {
        Some(Value::Number(number)) => number
            .as_u64()
            .or_else(|| number.as_i64().map(|value| value.max(0) as u64))
            .or_else(|| number.as_f64().map(|value| value.max(0.0) as u64))
            .unwrap_or(0),
        Some(Value::String(text)) => text.trim().parse::<u64>().unwrap_or(0),
        _ => 0,
    }
}

/// Independently validated explicitly reported output count.
///
/// Only a non-negative integer, or its supported numeric-string form, of the
/// selected raw `output_tokens` (falling back to `completion_tokens`) is
/// accepted. Missing, null, negative, fractional and malformed values yield
/// `None`, so none of them can masquerade as an explicitly reported zero, and
/// this result is independent of the input/cache decomposition validity.
fn reported_output_count(usage: &Value) -> Option<u64> {
    usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .and_then(strict_non_negative_integer)
}

fn strict_non_negative_integer(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.trim().parse::<u64>().ok(),
        _ => None,
    }
}

/// A nested cache field when its key is present with a non-null value.
/// A present field that parses to zero stays `Some(0)` so shape detection can
/// distinguish "present" from "absent"; `null` counts as absent.
fn nested_token(usage: &Value, object: &str, key: &str) -> Option<u64> {
    usage
        .get(object)
        .and_then(|details| details.get(key))
        .filter(|value| !value.is_null())
        .map(|value| token_number(Some(value)))
}

/// Strictly read one native DeepSeek cache count.
///
/// `Some(None)` means the key is absent or `null`; `Some(Some(n))` means a
/// present non-negative integer or its supported numeric-string form; `None`
/// means the key carries a present value that is not an acceptable count. The
/// generic [`token_number`] coercion is deliberately not reused for these new
/// fields, so a negative, fractional or malformed claim is never coerced.
fn native_cache_token(usage: &Value, key: &str) -> Option<Option<u64>> {
    match usage.get(key) {
        None => Some(None),
        Some(value) if value.is_null() => Some(None),
        Some(value) => strict_non_negative_integer(value).map(Some),
    }
}

fn top_field_present(usage: &Value, key: &str) -> bool {
    usage.get(key).is_some_and(|value| !value.is_null())
}

/// The conservative fallback for an invalid cache decomposition: the reported
/// inclusive input is billed once as ordinary with no cache tiers, so no token
/// is double counted and no ordinary count goes negative.
fn invalid_canonical_usage(
    reported: u64,
    output_tokens: u64,
    reported_output_tokens: Option<u64>,
) -> CanonicalUsage {
    CanonicalUsage {
        tokens: UsageTokens {
            input_tokens: reported,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            output_tokens,
        },
        valid: false,
        reported_output_tokens,
    }
}

/// Map an upstream `usage` object to the four exclusive token tiers.
///
/// Missing fields become 0; the caller decides whether usage was present at all.
///
/// Shape table:
/// - OpenAI Chat/Responses report the inclusive input total (`input_tokens`
///   wins over `prompt_tokens`) with the cached subset nested under
///   `prompt_tokens_details` / `input_tokens_details`. The stored ordinary
///   tier is the checked remainder `reported - read - write`; each tier is
///   billed exactly once by `total()` and `compute_cost_at_time_with_breakdown`.
/// - Native DeepSeek reports the inclusive input with `prompt_cache_hit_tokens`
///   as the cached subset and an optional `prompt_cache_miss_tokens` equal to
///   `reported - hit`. A present native hit is the fallback when nested details
///   are absent, must equal a coexisting nested read (counted once) and, like a
///   nested read, conflicts with the independent Anthropic read tier.
/// - Anthropic-style split has no nested cache details: the top-level input
///   is already ordinary and the flat `cache_read_input_tokens` /
///   `cache_creation_input_tokens` tiers are independent.
/// - Mixed compatible write (nested cache read present, no nested cache
///   write, top-level creation present) follows OpenAI inclusive semantics
///   with the top-level creation value as the deducted cache-write fallback.
/// - Nested cache write wins over the top-level creation fallback.
/// - `output_tokens` wins over `completion_tokens`.
pub(in crate::ai_gateway) fn canonical_usage_from_value(usage: &Value) -> CanonicalUsage {
    let reported = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .map(|value| token_number(Some(value)))
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .map(|value| token_number(Some(value)))
        .unwrap_or(0);
    // Independently validated reported output count: the selected raw
    // field (output_tokens before completion_tokens) must be a non-negative
    // integer or its numeric-string form; any other shape stays unknown.
    let reported_output_tokens = reported_output_count(usage);
    let nested_read = nested_token(usage, "prompt_tokens_details", "cached_tokens")
        .or_else(|| nested_token(usage, "input_tokens_details", "cached_tokens"));
    let nested_write = nested_token(usage, "prompt_tokens_details", "cache_write_tokens")
        .or_else(|| nested_token(usage, "input_tokens_details", "cache_write_tokens"));
    let has_nested = nested_read.is_some() || nested_write.is_some();
    let has_top_read = top_field_present(usage, "cache_read_input_tokens");
    let top_read = token_number(usage.get("cache_read_input_tokens"));
    let top_creation = token_number(usage.get("cache_creation_input_tokens"));

    // Native DeepSeek cache fields. A present value must be a
    // non-negative integer or its supported numeric-string form; `null` is
    // absent. A present but malformed value is an invalid decomposition.
    let (native_hit, native_miss) = match (
        native_cache_token(usage, "prompt_cache_hit_tokens"),
        native_cache_token(usage, "prompt_cache_miss_tokens"),
    ) {
        (Some(native_hit), Some(native_miss)) => (native_hit, native_miss),
        _ => return invalid_canonical_usage(reported, output_tokens, reported_output_tokens),
    };

    // Conflicting shape: a nested or native cache hit coexists with the
    // independent top-level cache-read tier. Never merge; fall back.
    if has_top_read && (nested_read.is_some() || native_hit.is_some()) {
        return invalid_canonical_usage(reported, output_tokens, reported_output_tokens);
    }

    // Resolve the exclusive cache-hit tier. The native hit is the fallback when
    // the nested read is absent; an equal nested+native pair counts the hit
    // once, while an unequal pair is a contradictory shape.
    let cache_read_tokens = match (nested_read, native_hit) {
        (Some(nested), Some(native)) if nested == native => nested,
        (Some(nested), None) => nested,
        (None, Some(native)) => native,
        (None, None) => 0,
        _ => return invalid_canonical_usage(reported, output_tokens, reported_output_tokens),
    };

    // A present native miss must equal `reported - selected hit`; a miss-only
    // field therefore never invents a cached count.
    if let Some(miss) = native_miss {
        if reported.checked_sub(cache_read_tokens) != Some(miss) {
            return invalid_canonical_usage(reported, output_tokens, reported_output_tokens);
        }
    }

    let has_inclusive_cache = has_nested || native_hit.is_some();
    if has_inclusive_cache {
        // Nested cache write wins; otherwise the top-level creation value is
        // the compatible fallback deducted from the inclusive total.
        let cache_write_tokens = nested_write.unwrap_or(top_creation);
        match reported
            .checked_sub(cache_read_tokens)
            .and_then(|rest| rest.checked_sub(cache_write_tokens))
        {
            Some(input_tokens) => CanonicalUsage {
                tokens: UsageTokens {
                    input_tokens,
                    cache_read_tokens,
                    cache_write_tokens,
                    output_tokens,
                },
                valid: true,
                reported_output_tokens,
            },
            // Illegal decomposition: cache components exceed reported input.
            // Never produce negative ordinary input via saturating subtraction.
            None => invalid_canonical_usage(reported, output_tokens, reported_output_tokens),
        }
    } else {
        // Anthropic-style split: the reported input is already ordinary.
        CanonicalUsage {
            tokens: UsageTokens {
                input_tokens: reported,
                cache_read_tokens: top_read,
                cache_write_tokens: top_creation,
                output_tokens,
            },
            valid: true,
            reported_output_tokens,
        }
    }
}

#[cfg(test)]
pub(in crate::ai_gateway) fn usage_tokens_from_value(usage: &Value) -> UsageTokens {
    canonical_usage_from_value(usage).tokens
}

/// Parse `usage` out of a complete (buffered) upstream JSON response, keeping
/// the canonical validity classification so the store can persist it.
pub(in crate::ai_gateway) fn parse_usage_from_response(body: &[u8]) -> Option<CanonicalUsage> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let usage = value.get("usage")?;
    if usage.is_object() {
        Some(canonical_usage_from_value(usage))
    } else {
        None
    }
}

/// Read-only SSE accumulator that extracts the last valid `usage` object from a
/// forwarded stream without touching the bytes written to the caller.
///
/// Both locations are recognized: Chat Completions carries `usage` at the event
/// top level, while Responses carries it nested under the completed response
/// (`response.completed` -> `response.usage`). Only a JSON object replaces the
/// last accumulated object, so a later `null`, string or number payload can
/// never overwrite it.
#[derive(Default)]
pub(in crate::ai_gateway) struct SseUsageAccumulator {
    buffer: String,
    usage: Option<CanonicalUsage>,
}

impl SseUsageAccumulator {
    pub(in crate::ai_gateway) fn feed(&mut self, chunk: &[u8]) {
        self.buffer.push_str(&String::from_utf8_lossy(chunk));
        while let Some(position) = self.buffer.find('\n') {
            let line: String = self.buffer.drain(..=position).collect();
            self.consume_line(line.trim_end_matches(['\r', '\n']));
        }
        // Guard against a pathological stream that never emits a newline: the
        // usage chunk arrives at the end, so only a bounded tail can matter.
        if self.buffer.len() > 256 * 1024 {
            self.buffer.clear();
        }
    }

    fn consume_line(&mut self, line: &str) {
        let Some(data) = line.strip_prefix("data:") else {
            return;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            return;
        };
        // Chat Completions reports usage at the event top level; Responses
        // reports it nested under the response. A non-object payload (`null`,
        // string, number) never replaces the last valid object.
        let usage = value
            .get("usage")
            .or_else(|| value.get("response").and_then(|response| response.get("usage")));
        if let Some(usage) = usage {
            if usage.is_object() {
                self.usage = Some(canonical_usage_from_value(usage));
            }
        }
    }

    pub(in crate::ai_gateway) fn usage(&self) -> Option<UsageTokens> {
        self.usage.map(|canonical| canonical.tokens)
    }

    /// The last valid usage object with its canonical validity classification.
    pub(in crate::ai_gateway) fn canonical_usage(&self) -> Option<CanonicalUsage> {
        self.usage
    }
}
