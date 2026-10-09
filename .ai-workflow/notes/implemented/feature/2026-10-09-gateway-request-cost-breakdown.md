# Agent Note: Gateway Request Cost Breakdown Snapshot

Status: implemented

English | [中文](2026-10-09-gateway-request-cost-breakdown.zh.md)

## Problem

The usage log froze one total `amount` per forwarded attempt row, so operators could see what a request cost but not how that total decomposed into the input, cache-read, cache-write and output tiers. The frozen total was computed at record time from the forwarded provider's own exact price row and could not be reconstructed later, because the applied effective tier, the unit rates and the per-tier fees were never stored; reading them back from the current price table would reprice history and knowingly disagree with the frozen `amount` whenever a rate or off-peak window changed. The request-log row therefore lacked the inspectable per-tier detail the next step needs.

## Decision

`UsageCostBreakdown` is a new public struct in `src-tauri/src/ai_gateway/usage_log.rs` carrying eight numeric fields: the resolved effective unit rates `input_price`, `output_price`, `cache_read_price` and `cache_write_price` in USD per million tokens, and the frozen fees `input_cost`, `output_cost`, `cache_read_cost` and `cache_write_cost` in USD. The total is deliberately not duplicated inside the struct: `UsageLogRecord.amount` stays the single authoritative total.

`UsageLogRecord` gains `cost_breakdown: Option<UsageCostBreakdown>` with `#[serde(default)]`, so a missing or `null` snapshot means the per-tier detail is unavailable rather than zero; an explicit zero fee is a real zero. The store persists it as a nullable `cost_breakdown TEXT` JSON column that is present in the fresh schema and appended by the existing idempotent, additive `migrate_usage_logs` path from `PRAGMA table_info`; the new column is never backfilled, and neither `migration.rs` nor the configuration schema, `user_version` or any version policy changes.

Public `compute_cost_at_time_with_breakdown(price, tokens, timestamp_ms) -> (f64, UsageCostBreakdown)` resolves the first matching UTC+8 off-peak tier exactly once and returns both the total and the snapshot, keeping the existing total's sum-before-division arithmetic and tier order unchanged. `compute_cost_at_time` delegates to it and returns `.0`, so existing callers and semantics are preserved.

`runtime_http::build_usage_log_row` performs the existing exact, case-sensitive provider plus upstream-model price match and, when a row matches, freezes the snapshot and the total together from that one resolution; no match stores `None` for both. The stored tokens come from the same actual attempt usage, so priced attempts, terminal failures and successes all carry a snapshot, and a matched row with zero rates or zero usage still records the actual rates with zero fees. The shared insert and projection helpers (`INSERT_SQL`, `insert_record`, `RECORD_COLUMNS` and `record_from_row`) carry the column through single-row and batch writes and through every record-producing query.

## Alternatives considered

- Recompute per-tier fees from `amount` and the current price table at read time: declined because editing a price or off-peak window would rewrite how past requests are shown and can disagree with the frozen `amount`.
- Persist only the four unit rates and derive the fees on read: declined because the fees must be the exact values produced by the same resolution that produced `amount`, and re-deriving them can drift from it.
- Add a redundant total inside the snapshot: declined because `UsageLogRecord.amount` is already authoritative and a second total invites divergence.
- Backfill historical rows from the current price table: declined because the historical effective tier and rates are unknowable, so a backfill would fabricate precision; unavailable stays `null`.
- Bump the usage database or configuration schema version for the new column: declined because a nullable additive column needs no version gate and the frozen contract forbids configuration schema and version changes.

## Consequences

- The row detail is inspectable without repricing: a stored snapshot is the same single tier resolution that produced `amount`, including the applied off-peak window.
- Missing snapshots from old rows or older payloads read back as `null` and mean unavailable, never zero; an explicit zero rate or fee is preserved as zero.
- The storage change is additive and nullable, so an older explicit-column client can still query and append rows, and a rollback leaves the column harmless with old rows `null`.
- Existing `amount`/metrics SQL and usage parsing are unchanged, and no current-configuration price lookup, backfill or rounding repair is introduced.
- This record is delivered backend-only by Step 1 of plan `20261009-gateway-request-cost-breakdown`; the frontend display is not implemented yet and is planned before this note's final revision.
- Partial supersession: [API Gateway Usage Stats and Request Logs](../architecture/2026-09-17-ai-gateway-usage-logs.md) is retained and cross-linked; this record adds the optional frozen `cost_breakdown` snapshot and its nullable column to the stored row fields, while that record's SQLite logging, record-time price freeze, retention, `group_by` contract and `local_model` facet decisions remain in force.
- `MEMORY.md` and the `ai-gateway-backend` entry in `navigation.json` describe the same type, helper, column and snapshot behavior, and `navigation.md` was regenerated from the authoritative JSON.
