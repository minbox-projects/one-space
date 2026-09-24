# Agent Note: API Gateway Cache Hit Rate Normalizes Provider Usage Semantics

Status: implemented

English | [中文](2026-09-21-api-gateway-cache-hit-accounting.zh.md)

## Problem

The usage panel derived a token cache hit rate in the frontend as `cache_read / (input + cache_read)`, but providers report incompatible input semantics. OpenAI Chat and Responses report an inclusive input total (`prompt_tokens` / `input_tokens`) that already contains the cached subset, while Anthropic-style upstreams report a top-level input that is already ordinary plus independent `cache_read_input_tokens` and `cache_creation_input_tokens` tiers. Adding those raw inputs together over-counted the denominator, so one local model served by different providers produced different, unexplained rates, and two providers that each reported `80%` combined into roughly `57%`. Legacy database rows, requests whose upstream returned no `usage` object and deliberately zero-denominator requests were indistinguishable from a genuine `0%`, so missing coverage masqueraded as a real miss rate.

## Decision

Every newly parsed upstream `usage` object is normalized by `canonical_usage_from_value` in `src-tauri/src/api_gateway/usage_log.rs` into four mutually exclusive tiers: `input_tokens` (ordinary input), `cache_read_tokens`, `cache_write_tokens` and `output_tokens`. `input_tokens` wins over `prompt_tokens` and `output_tokens` wins over `completion_tokens`; the inclusive OpenAI shapes subtract their nested cache components from the reported total with checked subtraction, so no cache token is counted twice.

| Shape | Detection and source fields | Normalization |
| --- | --- | --- |
| OpenAI Chat | `prompt_tokens` or `input_tokens` with nested `prompt_tokens_details.cached_tokens`; optional nested `prompt_tokens_details.cache_write_tokens` | `ordinary = reported − cache_read − cache_write` by checked subtraction |
| OpenAI Responses | `input_tokens` with nested `input_tokens_details.cached_tokens`; optional nested `input_tokens_details.cache_write_tokens` | The same checked subtraction as OpenAI Chat |
| Anthropic-style split | Top-level `input_tokens` or `prompt_tokens` with `cache_read_input_tokens` and `cache_creation_input_tokens`, without nested cache details | The reported input is already ordinary; cache read and cache write stay independent tiers |
| Mixed compatible write | Nested cache read, no nested cache write, plus top-level `cache_creation_input_tokens` | OpenAI inclusive semantics with the top-level creation value deducted as the cache-write fallback; a nested cache write wins over it |

Two shapes are invalid and are never repaired by saturating subtraction: a nested cache read that coexists with a top-level `cache_read_input_tokens`, and nested cache components that exceed the reported input. An invalid row keeps the reported input with no cache tiers so billing stays conservative and never negative or double counted, its cache tiers never participate in cache-tier pricing, and the request stays cache-statistics-ineligible. Historical amounts are never recomputed.

Storage is migrated additively and idempotently. `PRAGMA table_info` adds `usage_semantics` (pre-existing rows default to `legacy`, new rows are `canonical_v1`), `usage_present` and `cache_accounting_valid`; original tokens, totals, amounts and log fields are never rewritten, and legacy rows are excluded from the new numerator and denominator.

`UsageMetrics` gained five always-serialized additive fields at totals, buckets, models and providers: `cache_hit_tokens`, `cache_eligible_tokens`, `cache_hit_rate_percent` (a `0..=100` percentage or `null`), `cache_rate_eligible_count` and `successful_request_count`. The command name `api_gateway_usage_stats` and its existing fields are unchanged. The numerator is the summed `cache_read` and the denominator is the summed `ordinary + cache_read + cache_write`; one shared eligibility predicate serves every level (terminal, `result='success'`, `usage_semantics='canonical_v1'`, `usage_present=1`, `cache_accounting_valid=1`, positive denominator), so the same local model across providers sums numerator and denominator and computes the rate once, token-weighted, while provider rows stay separated by `local_model + provider_id + provider_name`. `successful_request_count` counts in-range successful terminal rows, `cache_rate_eligible_count` counts only eligible rows, legacy / missing / zero-denominator / invalid successes count as successful but not eligible, and failures and no-candidate rows count as neither; the legacy `cancelled` rows are deleted once when an older database first opens ([Gateway Migration Is Permanent and Version-Gated](../architecture/2026-09-24-version-gated-gateway-migration.md)).

`SseUsageAccumulator` accepts both the Chat Completions top-level `usage` and the Responses `response.completed.response.usage`, keeps the last valid object and reads forwarded bytes only with bounded passthrough; the gateway never injects `stream_options.include_usage`, and a missing usage object is recorded with `usage_present = false`. No request or response byte is mutated and `forwarding.rs` is unchanged. The usage panel renders the backend `cache_hit_rate_percent` directly (`null` shows `—`, a legitimate `0.0` shows `0%`), shows coverage as eligible / successful counts at totals, model and provider levels, and fixes the metric names to the Chinese “Token 缓存命中率” and English “Token Cache Hit Rate”.

## Alternatives considered

- Keep the frontend formula over raw token fields: declined because the provider input semantics are incompatible, so no client-side arithmetic can recover the correct rate, and the same model would keep aggregating differently per provider.
- Heuristically reclassify legacy rows as canonical: declined because the original field shape is lost in storage, so a heuristic could invent a plausible but wrong rate instead of exposing the coverage gap.
- Repair invalid or over-large nested cache components with saturating subtraction: declined because it fabricates an impossible decomposition, can double count a cache token and can produce negative ordinary input; the shape stays invalid with a conservative billing fallback.
- Inject `stream_options.include_usage` for Chat streaming requests to raise coverage: declined because it mutates the client request body and risks breaking compatible upstreams; missing usage is reported as not present instead.
- Render missing or invalid usage as `0%`: declined because a genuine uncached request with positive ordinary input is eligible and truly `0%`, while no eligible positive denominator must show `—` / `null`.
- Average precomputed provider percentages for a model or total: declined because percentages are not additive; summing numerator and denominator per level gives the token-weighted rate, so two providers at `80%` stay `80%`.

## Consequences

- Totals, buckets, model rows and provider rows share one backend-owned, token-weighted cache rate, so the same local model served by OpenAI-style and Anthropic-style providers aggregates correctly.
- `api_gateway_usage_stats` keeps its command name and existing fields; the five additive fields are always serialized at every level.
- The SQLite migration only adds columns, legacy rows stay visible in logs, totals and historical cost exactly as stored, and no historical amount is recomputed; a rollback can revert the parser, aggregation and UI while leaving the additive columns harmless.
- Coverage is explicit: `cache_rate_eligible_count / successful_request_count` distinguishes valid new data from legacy, missing, zero-denominator and invalid requests, and `null` is the only no-data representation while `0%` is reserved for a legitimate positive-denominator uncached request.
- Streaming forwarding bytes and request bodies stay unchanged; no `stream_options.include_usage` is injected and a missing usage object is recorded as not present.
- Partial supersession: [API Gateway Usage Stats and Request Logs](../architecture/2026-09-17-ai-gateway-usage-logs.md) is retained and cross-linked; this record replaces only its recorded streaming-usage source, its missing-usage-zero behavior and its raw token-tier semantics, while the SQLite logging, record-time price freeze, retention, `group_by` contract and `local_model` facet decisions there remain in force.
- `MEMORY.md` and both `api-gateway` navigation responsibilities describe this normalization, schema, eligibility and coverage contract, and `navigation.md` was regenerated from the authoritative JSON.
