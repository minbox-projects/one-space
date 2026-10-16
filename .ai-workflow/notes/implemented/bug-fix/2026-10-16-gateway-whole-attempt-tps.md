# Agent Note: Gateway Whole-Attempt Average TPS

Status: implemented

English | [中文](2026-10-16-gateway-whole-attempt-tps.zh.md)

## Problem

The request-log Average TPS divided one attempt's valid, explicitly reported cumulative output-token count by the unrounded first-output-to-end generation window. That divisor is not truthful on its own: the reported count is a property of the whole attempt, while the visible streaming window can be a small fraction of it, so a short window paired with a full count produced a rate hundreds of times too high. The measured attempt recorded 1739 output tokens over a 41.830 s whole attempt with a 41622.7707 ms first-token latency and stored 8449.763198666167 t/s, while the whole-attempt rate is 1739 / 41.83 = 41.57303370786517 t/s (displayed 41.57). The specific physical origin of the mismatch — for example transport buffering or hidden reasoning that the visible stream does not represent — is unproven and is not asserted here. The change also had to keep the existing nullable columns and old-code rows readable and had to keep an unmeasured value distinct from a measured zero.

## Decision

Average TPS is one attempt's valid, explicitly reported cumulative output-token count divided by the unrounded whole-attempt monotonic interval from the attempt start to the attempt end, not the first-output-to-end generation window. First-token latency is unchanged: it remains the start-to-first-meaningful-output interval, and the whole-attempt interval now also contains the pre-first-output wait. No arbitrary TPS cap, floor or threshold is applied.

A rate still requires a measurable attempt: meaningful upstream output must have been observed and the reported output count must be valid. When the whole interval is zero or negative there is no TPS; a first output that coincides with the attempt end is still measurable as long as the whole interval is positive. An explicit zero count over a positive interval is `0.00 t/s`, while a missing, null or malformed count stays unknown and never becomes zero.

Persistence is unchanged in shape: the same serde-default `first_token_ms` and `avg_tps` fields and the same nullable `first_token_ms REAL` and `avg_tps REAL` columns. Reads reproject the rate instead of trusting the stored number: in `record_from_row`, a row whose stored `avg_tps` is non-null is projected as `output_tokens / (duration_ms / 1000)` when `duration_ms` is positive, and a null stored `avg_tps` stays unavailable. The stored column, the database bytes and the schema are never rewritten, `first_token_ms` is projected as stored, and no historical value is backfilled. Non-streaming eligibility, the meaningful-output classification, the per-attempt observer lifecycle and the billing, cost and cache arithmetic are unchanged.

## Alternatives considered

- Keep the first-output-to-end generation-window divisor: declined because a visible window that is short relative to the reported count overstates the rate by orders of magnitude, as the 8449.76 t/s versus 41.57 t/s logged attempt shows; the reported count is a whole-attempt quantity and needs a whole-attempt denominator.
- Cap, clamp or floor TPS at a threshold: declined because any such bound would present an invented number instead of the real quotient; the fix reports the actual rate and leaves an unmeasurable attempt unavailable.
- Backfill or rewrite historical rows, or add a migration or schema version: declined because the defect is only in how a rate is derived, so the stored column and the database are left untouched and historical rows are reprojected at read instead.
- Derive a token count or a rate from text length when the reported count is missing: declined because text length is not a reported count and inventing one would fabricate data the gateway never observed; an unknown count stays unknown.

## Consequences

- The request-log Average TPS now reflects the whole attempt; a short visible streaming window no longer inflates it, and the logged attempt shows 41.57 t/s instead of 8449.76 t/s. First-token latency, the meaningful-output rules, the nullable columns and the frontend three-line display are unchanged.
- A stored measured row is reprojected at read as `output_tokens / (duration_ms / 1000)`; the stored `avg_tps`, the database bytes and the schema are unchanged, a row without a stored `avg_tps` stays unavailable, and a measured zero stays distinct from unknown.
- Partial supersession: this record supersedes only the average-TPS denominator and the related read-projection expectation of [Gateway Request Latency: Observed First-Token and Generation TPS](../feature/2026-10-14-gateway-request-latency.md). That record is retained and cross-linked, and its first-token, observer, column, rendering and color facts remain current.
- Delivery status: the behavior tests now pass — `cargo test --manifest-path src-tauri/Cargo.toml ai_gateway::tests::output_timing` reports 39 passed and 0 failed, and `ai_gateway::tests::usage_store` reports 17 passed and 0 failed with one pre-existing ignored performance test; the regression fixture reprojects 41.573 t/s while the raw stored column stays 8449.763, and first-token latency plus the null, reopen, filter and three-protocol stream behavior are covered. No merge, release or commit is claimed.
