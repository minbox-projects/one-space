# Agent Note: Gateway Request Latency: Observed First-Token and Generation TPS

Status: implemented

English | [中文](2026-10-14-gateway-request-latency.zh.md)

## Problem

The request-log list showed one whole-attempt `duration_ms` and one duration color per row. That single number could not separate a model that was slow to produce its first token from one that streamed slowly afterwards, and it gave no output rate. Deriving a rate from the rounded display duration would misreport it, and estimating a first-token time for a buffered non-streaming response or tokens from text length would fabricate data the gateway never observed. The change also had to preserve an older database and old-code inserts, and to keep "no measurement" distinguishable from a measured zero.

## Decision

First-token latency and average TPS are measured per upstream attempt by a read-only `OutputTimingObserver` (`src-tauri/src/ai_gateway/output_timing.rs`). It is created fresh at the attempt's `tokio::time::Instant` start and fed the same raw upstream chunks already used for usage capture, before any awaited downstream write, so it observes the actual upstream protocol before conversion; it is finished before health settlement, persistence and downstream completion, so the observed interval excludes bookkeeping time. First-token latency is the monotonic interval from the attempt start to the first meaningful upstream output; average TPS is the attempt's valid, explicitly reported cumulative output-token count over the unrounded whole-attempt interval, revised by [Gateway Whole-Attempt Average TPS](../bug-fix/2026-10-16-gateway-whole-attempt-tps.md) within the same delivery.

Meaningful output is non-empty text, reasoning/thinking, a generated tool or function name, or generated tool arguments, interpreted per actual upstream protocol: Chat Completions (including the deprecated `delta.function_call` name/arguments), Responses output-text/reasoning/function-argument deltas and a function-call item that carries a name or arguments, and Anthropic Messages text/thinking/input-JSON deltas plus an initial `content_block_start` text, thinking or `tool_use` block with content. A heartbeat or comment, an empty delta, a role-only or other metadata event, a usage-only event, an error event, a completion marker and an identifier-only tool envelope are not output. Whitespace inside a non-empty string counts as output and is never trimmed. Incremental SSE buffering handles LF and CRLF framing, multiline `data:` lines, fragmented events and a final valid event that arrives without its blank-line terminator, which is recognized at finish.

The reported output count is validated independently of input/cache accounting: `CanonicalUsage.reported_output_tokens` selects `output_tokens` before `completion_tokens` and accepts only a non-negative integer or its supported numeric-string form. Missing, null, negative, fractional and malformed values stay unknown and can never become zero, while a valid non-negative integer is accepted even when the input/cache decomposition is invalid. Billing token tiers, costs and cache-validity are unchanged. An explicit zero count over a measured positive interval yields `0.00 t/s`; a missing count, a missing first observation or a non-positive generation interval yields no TPS; a measured zero first-token offset stays a valid observation.

`UsageLogRecord` gains the serde-default optional fields `first_token_ms` and `avg_tps`, and `usage_logs` gains the nullable `first_token_ms REAL` and `avg_tps REAL` columns through the existing idempotent additive-column migration; both are projected in single and batch inserts and in filtered and paged queries. No historical value is backfilled. Non-streaming, synthetic no-upstream, no-output and pre-output-failure rows, and rows written by older code, keep both columns null. `duration_ms` keeps its existing millisecond resolution.

The ungrouped request-list column is labeled Latency / 延迟 and shows three aligned lines — Total / 总耗时, First token / 首字 and Average TPS / 平均 TPS — with three-decimal seconds and two-decimal TPS, visible muted `s` and `t/s` units, right-aligned tabular values and muted labels; only the numeric value carries color and an unavailable value renders a single muted `—`. Colors use independent segments on the original value before display rounding:

| Metric | Green | Orange | Red |
| --- | --- | --- | --- |
| Total duration (`s`) | `< 3` | `3 <= value <= 15` | `> 15` |
| First token (`s`) | `< 1` | `1 <= value <= 3` | `> 3` |
| Average TPS (`t/s`) | `>= 60` | `20 <= value < 60` | `< 20` |

The frontend helpers live in `src/lib/aiGateway.ts`: `formatGatewayDuration` (milliseconds to three-decimal seconds), `formatGatewayTps`, `getGatewayDurationColorClass`, `getGatewayFirstTokenColorClass` and `getGatewayTpsColorClass`; each color helper returns muted foreground for null, undefined, non-finite or negative input and otherwise a readable light `*-600` plus dark `*-400` variant. `LatencyMetricRow` in `UsageLogsPanel.tsx` renders one aligned line, and the bilingual keys `aiGatewayLogsLatencyTotal`, `aiGatewayLogsLatencyFirstToken` and `aiGatewayLogsLatencyAverageTps` carry the labels while the column key stays `aiGatewayLogsDurationColumn`.

## Alternatives considered

- Treat the first transport bytes of the attempt as the first-token observation: declined because a comment heartbeat, a role-only or empty delta, a usage-only event or a lifecycle envelope arrives before real output, so an early chunk would inflate the apparent rate; the observation waits for the first complete, valid, meaningful event.
- Divide the output count by the whole attempt duration or by the rounded or clamped `duration_ms`: originally declined in favor of the unrounded first-output-to-end interval, with the independent arithmetic examples `200 / (5 - 1) = 50`, `200 / 3.5` and `200 / 0.0005 = 400000`; that choice was later reversed, and [Gateway Whole-Attempt Average TPS](../bug-fix/2026-10-16-gateway-whole-attempt-tps.md) now makes the whole-attempt interval the divisor.
- Invent a first-token time or a TPS for historical, non-streaming, synthetic no-upstream or no-output rows, or estimate tokens from text length: declined because a buffered response cannot reconstruct streaming timing and text length is not a reported count; unavailable stays null and renders a muted `—`.

## Consequences

- The ungrouped request list shows three aligned Total / First token / Average TPS lines with independent color segments in Chinese and English and in light and dark themes; the grouped views, the other columns, the filters and pagination are unchanged.
- `first_token_ms` and `avg_tps` are nullable columns and optional serde-default fields; reopening an older database adds the columns and keeps every existing row, old-projection inserts still succeed with both columns null, and a measured zero first-token offset or a zero TPS stays distinct from an unavailable value on the frontend.
- The rate is the whole-attempt output rate now defined by [Gateway Whole-Attempt Average TPS](../bug-fix/2026-10-16-gateway-whole-attempt-tps.md); it excludes input and cache tokens, retry waits and earlier attempts, and it never changes token-tier, cost or cache-hit-rate arithmetic.
- A failed stream that already produced output keeps its first-token measurement and the TPS derived from the cumulative output usage it actually observed; a downstream cancellation still discards the whole buffered attempt set and writes no rows.
- Partial supersession: [Gateway Whole-Attempt Average TPS](../bug-fix/2026-10-16-gateway-whole-attempt-tps.md) supersedes only this record's average-TPS denominator and read-projection expectation, and the rest of this record stays current. [API Gateway Usage Stats and Request Logs](../architecture/2026-09-17-ai-gateway-usage-logs.md) and [Gateway Per-Attempt Request Logging and Stored Error Text](../architecture/2026-09-20-gateway-per-attempt-logging-and-error-text.md) are retained and cross-linked; this record extends their per-attempt row, nullable-column and privacy contract with the first-token and timing measurements, and neither is rewritten or archived.
- `MEMORY.md` and both the `ai-gateway` and `ai-gateway-backend` navigation entries record the observer path, the new test path and the public helpers, and `navigation.md` is regenerated from the authoritative JSON in the same change.
