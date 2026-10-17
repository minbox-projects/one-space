# Agent Note: OpenAI Reasoning Controls Are Translated in the Anthropic Messages Conversion

Status: implemented

English | [中文](2026-10-17-openai-reasoning-anthropic-conversion.zh.md)

## Problem

When an OpenAI-ingress request (`/v1/chat/completions` or `/v1/responses`) is converted to a native Anthropic Messages upstream, `reject_common_conversion_fields` in `src-tauri/src/ai_gateway/anthropic.rs` treated every nonempty reasoning option as untranslatable and rejected it field-specifically with a 400 before any Anthropic attempt. A caller that set a Chat `reasoning_effort`/`reasoningEffort` or a Responses `reasoning.effort` could therefore not reach a Messages upstream at all, and a client such as Codex that always sends the output-presentation member `reasoning.summary` was blocked by a control that never changes reasoning depth. The native Messages upstream accepts `output_config.effort` and `thinking`, so the recognized client controls needed translation instead of rejection.

## Decision

- Chat recognizes the top-level `reasoning_effort` first and then the compatibility spelling `reasoningEffort`; the first present non-blank string wins, matching the existing `runtime_http::extract_reasoning_effort` log semantics. `null` and blank or whitespace-only values mean no control, and a present non-string value is a field-named 400.
- Responses recognizes the `reasoning` object: absent or `null` means no control, a non-object is a field-named 400, `effort` follows the same value rules with errors named `reasoning.effort`, and `summary` is accepted and ignored when it is a string or `null` while any other shape is a 400 named `reasoning.summary`. Any other member is a field-named 400, for example `reasoning.mode`.
- The private `insert_reasoning_effort` table translates the value: `low`/`medium`/`high`/`xhigh`/`max` become `output_config: {"effort": <same value>}`, `minimal` becomes `output_config: {"effort": "low"}` and `none` becomes `thinking: {"type": "disabled"}`. Any other value, including `auto` and a case variant, is a field-named 400 naming `unsupported reasoning effort '<value>'`.
- Translation runs inside the existing pre-attempt `validate_conversion_request` path, so an unsupported control still fails before any upstream attempt and settles no key health.
- The private helpers are `insert_reasoning_effort`, `apply_chat_reasoning_control` and `apply_responses_reasoning_control`; `reject_common_conversion_fields` no longer handles reasoning and lost its `chat` parameter.
- No public symbol, configuration schema, persistent format or pool path changes: a pool request still strips its recognized entry controls before conversion and injects the operator-declared native control first.

## Alternatives considered

- Silently drop the control and forward the request: declined because a caller asking for more or less reasoning would receive the upstream default and its explicit intent would be misrepresented.
- Pass the client value through verbatim without the `minimal` and `none` mapping: declined because Anthropic has no `minimal` level and no top-level `reasoning_effort` field, so a verbatim value would be rejected or ignored by the upstream instead of expressing the requested depth.
- Map the level to `thinking.budget_tokens`: declined because the caller supplies a level rather than a token budget, and inventing a budget would fabricate a value the caller never chose; the level maps to `output_config.effort` instead.
- Reject the Responses `reasoning.summary` member: declined because summary is an output-presentation control that never changes reasoning depth, the conversion cannot deliver reasoning summaries anyway, and refusing it would block clients such as Codex that always send it.
- Keep rejecting every reasoning option as before: declined because the native Messages upstream can honor the recognized levels through `output_config` and `thinking`, so the blanket 400 blocked a capability the upstream supports.

## Consequences

- The changed paths are `src-tauri/src/ai_gateway/anthropic.rs` (+122/-21) and `src-tauri/src/ai_gateway/tests/anthropic_conversion.rs` (new tests plus one comment).
- The delivered translation is:

| Client reasoning control | Native Anthropic control |
| --- | --- |
| Chat `reasoning_effort`/`reasoningEffort` or Responses `reasoning.effort` equal to `low`/`medium`/`high`/`xhigh`/`max` | `output_config: {"effort": <same value>}` |
| That field equal to `minimal` | `output_config: {"effort": "low"}` |
| That field equal to `none` | `thinking: {"type": "disabled"}` |
| Responses `reasoning.summary` (string or `null`) | Accepted and ignored |
| An unknown value (including `auto` and case variants), a non-string, a non-object `reasoning` or an unknown member | A field-named 400 before any attempt |

- Test evidence: `cargo test --manifest-path <worktree>/src-tauri/Cargo.toml --lib ai_gateway::tests::anthropic_conversion` exited 0 with 40 passed, and `cargo test --manifest-path <worktree>/src-tauri/Cargo.toml --lib ai_gateway::` exited 0 with 971 passed and the two pre-existing ignored `core_workflows_perf_*` measurement tests.
- This record partially supersedes [Native Anthropic Messages Entry, Protocol Recognition and OpenAI Entry Conversion](../architecture/2026-10-11-ai-gateway-anthropic-messages.md) only for its "a nonempty untranslatable reasoning option is rejected field-specifically with a 400" boundary; that record is retained and cross-linked, and all its other decisions remain in force.
- This record partially supersedes [Model Pools Own a Bounded Pool-Local Reasoning Policy](../feature/2026-10-15-pool-only-reasoning.md) only for its "`anthropic.rs` stays unchanged so the ordinary Anthropic adapter keeps its rejection rules" boundary; that record is retained and cross-linked, and all its other decisions remain in force.
