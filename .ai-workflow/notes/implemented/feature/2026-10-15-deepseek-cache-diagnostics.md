# Agent Note: Native DeepSeek Cache Usage Is Normalized and the Chat Forwarding Contract Is Verified

Status: implemented

English | [中文](2026-10-15-deepseek-cache-diagnostics.zh.md)

## Problem

DeepSeek's native Chat Completions response reports an inclusive `prompt_tokens` together with a native cached subset in `prompt_cache_hit_tokens` and an optional `prompt_cache_miss_tokens`. `canonical_usage_from_value` recognized nested OpenAI cache details and Anthropic split tiers but did not consume the native fields, so a native-only response with inclusive input `100`, hit `98` and miss `2` was stored as `100` ordinary input tokens, `0` cache-read tokens, a `0%` cache hit rate and an input-tier charge, even though the equivalent nested shape was already understood. That gap left the gateway unable to account for reported cache hits without also confirming that the forwarding path preserved the conversation payload and did not itself introduce a cache loss.

## Decision

`canonical_usage_from_value` in `src-tauri/src/ai_gateway/usage_log.rs` now also normalizes the native DeepSeek cache fields into the existing four exclusive tiers, reusing the checked canonical decomposition instead of adding a parallel code path. A present native value must be a non-negative integer or the already supported integer-string form; `null` and an absent key mean absent, and an explicit `0` is present. The legitimate shapes and their normalization are:

| Native and nested shape | Detection | Normalization |
| --- | --- | --- |
| Native-only | present `prompt_cache_hit_tokens`, no nested cache details | the native hit becomes the cache-read tier |
| Consistent dual | native hit and nested `cached_tokens` both present and equal | the hit is counted once |
| Nested-only | only nested `cached_tokens` | the pre-existing nested normalization is unchanged |

A present native miss must equal the reported inclusive input minus the selected cache hit, so a miss-only response never invents a cached count. A native value that is negative, fractional or malformed, a native/nested hit mismatch, a miss amount inconsistent with `input - hit`, and a native or nested hit that coexists with the independent Anthropic `cache_read_input_tokens` tier all produce the existing conservative fallback: the reported input is billed once as ordinary with no cache tiers and the request stays cache-statistics-ineligible, and no failing shape is repaired by saturating subtraction. `input_tokens` still wins over `prompt_tokens` for the inclusive total, and the output-count validation stays independent of cache validity.

The result is identical for a buffered JSON response and for an incrementally observed fragmented SSE stream, missing usage remains `usage_present = false` rather than a fabricated `0%`, and no configuration schema, stored row or price snapshot is changed or backfilled. No public symbol was added; the native helpers stay private to `usage_log.rs`.

The same slice characterizes the existing Chat forwarding contract without changing it: model-only rewriting, native messages, ordered tool definitions, `reasoning_content` including empty strings and explicit nulls, tool calls and results, a supplied `user_id`/`metadata` and the recognized session headers survive gateway mapping, and upstream authentication retains its existing treatment. `forwarding.rs` and `selection.rs` needed no fix; a newly discovered defect would have required a bounded follow-up rather than an unapproved edit.

The behavior tests live in `src-tauri/src/ai_gateway/tests/deepseek_cache.rs`, registered as `mod deepseek_cache;` in `src-tauri/src/ai_gateway/tests.rs`, and cover the native-only, nested-only and consistent dual shapes, the conflicting, impossible and miss cases, explicit zero versus null, the integer-string form, `input_tokens` precedence, output-count independence, fragmented SSE, buffered and streamed persistence, no-double-counting and preserved response bytes.

## Alternatives considered

- Read the native fields as a second fallback after the independent Anthropic `cache_read_input_tokens` tier: declined because a native hit and that independent read describe the same cached quantity, so combining them would double count, and the shape is conflicting rather than complementary.
- Reuse the generic `token_number` coercion for the native fields: declined because it would silently coerce a negative, fractional or malformed claim into a plausible count instead of marking the accounting invalid.
- Derive a cache-read tier from `input - prompt_cache_miss_tokens` when only the miss is present: declined because that invents a hit the upstream did not report; a miss-only response must not create a cached count.
- Repair an inconsistent miss or an over-large hit with saturating subtraction or clamping: declined because it fabricates an impossible decomposition and can double count or negative-count tokens; the shape stays invalid with a conservative billing fallback.
- Edit `forwarding.rs` or `selection.rs` to chase the suspected cache loss: declined because the characterization showed the existing contract already preserves the payload, so any change would be an unapproved edit outside the delivered evidence.

## Consequences

- Newly appended request-log rows use the corrected native accounting; existing rows, historical tokens, amounts and price snapshots are untouched and never backfilled.
- The four exclusive tiers, the stored `usage_semantics`/`usage_present`/`cache_accounting_valid` columns, the shared eligibility predicate and the token-weighted cache-hit rate are unchanged; the native fields only feed the existing cache-read tier.
- Only private helpers were added, so the navigation entries changed for paths and semantics, not for new public symbols.
- Partial supersession: [API Gateway Cache Hit Rate Normalizes Provider Usage Semantics](../bug-fix/2026-09-21-api-gateway-cache-hit-accounting.md) is retained and cross-linked; this record extends its `canonical_usage_from_value` shape table with the native DeepSeek hit/miss fields and adds the forwarding characterization, while its four-tier normalization, storage migration, eligibility predicate and coverage contract remain in force.
- No supersession of routing: [Gateway Session Affinity Pins a Session and Model to One Upstream](../architecture/2026-09-20-gateway-session-affinity-routing.md) is retained and cross-linked; this change extends native-field accounting and diagnostics and does not alter the affinity policy, its header precedence, its binding rules or its process-memory-only state.
- Step 2 and Step 3 are not delivered by this record: the standalone observation/comparison tool does not exist yet, none of the planned `CACHE_DIAG_*` environment variables are defined, and no live comparison has run, so the 98% token-weighted effectiveness target remains unmeasured and is not claimed as met. This record will be extended as those steps land.
