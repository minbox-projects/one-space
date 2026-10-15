# Agent Note: Model Pool Portable Contract Recognizes the Assistant reasoning_content Trace

Status: implemented

English | [中文](2026-10-14-model-pool-reasoning-content.zh.md)

## Problem

A Chat Completions client that stores an assistant turn's provider-specific `reasoning_content` replays it on the next multi-turn history request; a normal client can replay the value as a string (including empty) or `null`, and current research shows OpenCode emits and round-trips the empty and nonempty field. The pool portable contract treated every unrecognized message key as unsupported, so `openai_conversion::validate_chat_message` returned a field-named `400` before any candidate was selected. A normal client history replay therefore failed even though the field is provider output rather than a caller control, and a same-protocol Chat candidate could already carry the text/tool history faithfully.

## Decision

- `openai_conversion::validate_chat_message` admits exactly one provider-specific message field beside the portable `role`/`content`/`tool_calls`/`tool_call_id` keys: assistant-only `messages[].reasoning_content`, whose value must be a string (empty, whitespace or nonempty) or `null`. It stays a trace rather than a control surface, so a non-assistant role or any other JSON type is still rejected by field before any candidate, SWRR mutation, attempt or probe.
- A same-protocol Chat pool candidate keeps the field exactly. `forwarding.rs` rewrites only the model for that case, so the incoming `messages` array reaches the upstream as the same exact JSON values — fields, values, key presence and array order are preserved, while raw whitespace and key-order bytes are not — including a present `reasoning_content` key whose value is `null`.
- Both conversions omit the trace without fabricating reasoning: `chat_request_to_responses` builds the canonical stateless text/tool input and never emits a Responses `reasoning` item, and the existing Chat-to-native Anthropic Messages adapter likewise emits no `reasoning` block. The provider-specific text never leaks into visible message content.
- `reasoning_effort` and `reasoningEffort` on Chat, opaque Responses `reasoning` state, an unresolved `item_reference` and every unlisted field keep their existing field-named rejections. No schema, public symbol, provider capability guess or session-affinity behavior is added.

## Alternatives considered

- Reject `reasoning_content` as before and require every client to strip it: declined because making each client customize its history serialization would break a normal assistant replay, while a same-protocol Chat candidate already carries the field and the adapter recognizes this single known exception; the trace is preserved on that protocol only and is not portable to the opposite protocol.
- Map arbitrary plaintext `reasoning_content` to an encrypted OpenAI Responses `reasoning` item: declined because plaintext has no faithful opaque-reasoning mapping, so it would fabricate provider state.
- Carry the trace onto a converted request: declined because the opposite protocol has no common representation for it and inserting it into visible content would corrupt the answer.
- Accept the field on every role and any JSON shape: declined because the field is an assistant-only typed trace, so the assistant-only role check and the string-or-null type check reject an invalid role or an invalid trace value.

## Consequences

- Reading a stored Chat assistant `reasoning_content` (string or `null`) no longer aborts a history replay with a `400`; the same-protocol Chat path delivers the exact history, and the Responses and native Messages paths stay canonical with no leaked or fabricated reasoning.
- Limitation: a provider-specific `reasoning_content` sent Chat-to-Chat still reaches the selected upstream, which may reject a field its own model does not know; the gateway makes no general cross-model reasoning-continuity promise.
- Verification: the four new `src-tauri/src/ai_gateway/tests/openai_conversion.rs` regressions pass (module filter `cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway::tests::openai_conversion::`, 4 passed, exit 0). On the shared build with no later product change, the full `openai_conversion` suite passes (42 passed, exit 0, including those four), native `anthropic_conversion` passes (36 passed, exit 0) and backend `model_pools` passes (22 passed, exit 0) — 100 unique tests across the three suites.
- Partial supersession: [Model Pools Route One Public Alias Across Explicit Source Candidates](../feature/2026-10-13-free-model-pool.md) is retained and cross-linked; this record adds only the single recognized assistant `reasoning_content` trace exception to its portable-contract bullet, and every other decision in that record still stands. This record is in turn partially superseded by [Model Pools Own a Bounded Pool-Local Reasoning Policy](../feature/2026-10-15-pool-only-reasoning.md) only where this record's formerly blanket entry-control rejection is narrowed by the newly recognized reasoning controls; this record's same-protocol Chat exact-JSON preservation of the assistant `reasoning_content` trace and its omission without fabrication on the Responses and native Messages conversions — all four trace behaviors — are retained.
