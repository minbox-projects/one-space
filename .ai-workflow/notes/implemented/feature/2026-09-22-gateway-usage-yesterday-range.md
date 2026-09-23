# Agent Note: API Gateway Usage Queries Replace Day Counts with a Named Range Selector

Status: implemented

English | [中文](2026-09-22-gateway-usage-yesterday-range.zh.md)

## Problem

The two gateway query commands encoded their quick ranges as an optional day count: `api_gateway_usage_stats(days: Option<i64>)` and `api_gateway_request_logs(days: Option<i64>, group_by, status, model, page)` sent `days = null` for all time and `Some(1)`, `Some(7)`, `Some(15)`, `Some(30)` for today and the last N natural days including today. The operator asked for a yesterday range, and that encoding cannot express it: every `Some(n)` window starts at today 00:00 UTC+8 and ends at tomorrow 00:00, so the half-open yesterday window `[yesterday 00:00, today 00:00)` is not a suffix of any supported window, and `Some(0)`, an absent value and negative values already mean all time, so no numeric value is free to carry yesterday. The hour-versus-day bucket choice must also follow the window, because a one-day window is displayed as an hourly distribution while a multi-day window is displayed by natural day.

## Decision

Both commands now take a trimmed string selector instead of a day count: `api_gateway_usage_stats(range: Option<String>)` and `api_gateway_request_logs(range: Option<String>, group_by, status, model, page)`. The vocabulary is `today`, `yesterday`, `7d`, `15d`, `30d` and `all`; `None`, an empty string and `all` are the unbounded all-time window, and an unknown selector is rejected with an actionable error listing the supported vocabulary and never falls back to all time or today. `yesterday` is the UTC+8 half-open window `[yesterday 00:00, today 00:00)`, and `7d` / `15d` / `30d` keep the previous windows unchanged (today plus the previous N-1 natural days, ending at tomorrow 00:00 UTC+8). Bucket granularity is derived from the resolved window: exactly one UTC+8 natural day (`today` and `yesterday`) serializes `granularity = "hour"`, and N-day and all-time windows serialize `granularity = "day"`. The frontend shares one six-entry range list — today, yesterday, 7d, 15d, 30d, all — between the usage-stats and request-logs panels with today as the default; `UsageRangeKey` gains `yesterday`, the unused `usageRangeToDays` helper is removed, the new i18n key `apiGatewayRangeYesterday` reads Yesterday / 昨天, and the runtime status card's today-statistics calls keep sending `range: "today"`. No database schema, persistence or aggregation semantics change; the command names and response shapes stay as they are.

## Alternatives considered

- Keep `days` and add a boolean or offset parameter for yesterday: declined because that leaves two encodings of one concept in a single signature, with a second branch to keep aligned and an ambiguous precedence between the parameters; one named selector carries every range the UI offers.
- Filter a today or all-time result on the client, or fetch two windows and subtract them: declined because range resolution, aggregation and the fixed pagination are backend-owned and pinned to UTC+8; a client-side window would re-implement day boundaries, break the backend totals and double the query cost for one selector value.
- Encode yesterday as `days = 0` or a negative day count: declined because `0`, absent and negative values already mean all time under the old normalization, so redefining them would silently change existing callers and leave the selector vocabulary ambiguous.
- Return hourly buckets for every range, or add a separate granularity parameter: declined because the bucket resolution is a pure consequence of the resolved window, and a caller-requestable granularity could contradict the window and produce empty or misleading distributions.

## Consequences

- The command signatures change: `api_gateway_usage_stats` now takes `range`, and `api_gateway_request_logs` takes `range` in place of `days`; `MEMORY.md`, `docs/USAGE.md` and the `api-gateway` / `api-gateway-backend` navigation entries describe the six selectors and the window-derived granularity, and `navigation.md` is regenerated from the authoritative JSON.
- The frontend keeps one shared `UsageRangeKey` / `USAGE_RANGE_KEYS` list for both panels, so the two tabs cannot drift apart, and the removed `usageRangeToDays` helper has no callers left.
- No database schema, persistence, aggregation or amount semantics change: `api_gateway_usage.db`, the retention behavior and both response shapes are untouched, and only the resolved window and the bucket labels differ.
- Rollback restores the previous signature and its call sites: an older build cannot read `range`, so the `days` parameters and the numeric callers must be restored together; no stored data needs migration or rewriting.
- Supersession: partial. [API Gateway Usage Stats and Request Logs](../architecture/2026-09-17-ai-gateway-usage-logs.md) recorded `days = null` as all time and `Some(1)` as today; that range-encoding sentence is replaced by this record, while its storage, recording, pricing and retention decisions remain in force.
