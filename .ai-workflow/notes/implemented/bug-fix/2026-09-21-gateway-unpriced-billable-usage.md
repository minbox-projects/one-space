# Agent Note: Gateway Unpriced Hint Counts Only Billable Usage

Status: implemented

English | [中文](2026-09-21-gateway-unpriced-billable-usage.zh.md)

## Problem

The usage-stats unpriced hint counted every terminal request that reached an upstream model without a matching price row, including terminal failures that recorded no usage at all. A request that consumed nothing and cost nothing therefore still demanded a price row, `unpriced_count` and `unpriced_items` overstated the requests that were genuinely missing a price, and a usage row whose requests were all zero-usage failures kept rendering `—`. The gateway does record failures with real usage — a streaming attempt that fails after its first forwarded byte is stored as a status-502 `failure` that keeps its already accumulated usage — so reaching an upstream model alone does not mean a request would incur usage-based cost.

## Decision

A request is unpriced only when all three hold: the stored `amount` is `NULL` (no matching price row), `upstream_model` is non-empty (the request reached an upstream model), and the request would incur usage-based cost — `result = 'success'`, or a recorded `total_tokens > 0`. The identical predicate `amount IS NULL AND upstream_model <> '' AND (result = 'success' OR total_tokens > 0)` is applied at both SQL sites in `src-tauri/src/api_gateway/usage_log.rs`: the unpriced aggregate of the shared `metric_columns()` projection that feeds totals, time buckets, per-model rows and per-provider rows, and the `unpriced_items` query that lists the affected model and provider rows.

The caliber is layered on purpose. Request counts still cover every terminal row, and tokens and amounts still aggregate what each row actually recorded, including failures that retained partial usage; only the unpriced hint narrows to the rows that would incur a cost. Zero-usage failures and no-upstream failures keep the `None` amount of an unpriced row but count as zero cost instead of unpriced. Behavior coverage: `usage_stats_unpriced_eligibility_requires_success_or_usage` asserts that a zero-usage failure is excluded, a partial-usage failure is included and a zero-usage success is included; `all_candidates_failed_request_writes_one_row_per_completed_attempt` expects an unpriced count of 0 because every attempt there is a zero-usage failure.

## Alternatives considered

- Count only `result = 'success'`: declined because a streaming attempt that fails after its first forwarded byte is recorded as a status-502 `failure` while keeping its accumulated usage, so its tokens and frozen amount are genuinely billable and dropping it would hide a model that needs a price row.
- Count only `total_tokens > 0`: declined because a zero-usage success still proves the model is in use, so the operator still needs a price row before later usage can be priced.
- Keep the previous predicate (any terminal row that reached an upstream model without a matching price row): declined because it reports zero-cost requests as unpriced and produces false hint noise, most visibly for zero-usage failures.

## Consequences

- The hint count now matches the requests that would incur usage-based cost: `unpriced_count`, `unpriced_items` and the `—` display exclude zero-usage failures and no-upstream failures while keeping zero-usage successes and usage-bearing failures.
- A usage row whose requests are all zero-usage failures no longer renders `—`; its aggregated amount is zero and displays as `0.0000` in the `Cost ($)` column.
- Request counts, tokens, amounts and the terminal rules are unchanged: `COUNT(*)`, token and amount aggregation, retention and per-attempt rows keep their existing behavior; the legacy `cancelled` rows are deleted once when an older database first opens, so no cancelled-row exclusion is needed ([Gateway Migration Is Permanent and Version-Gated](../architecture/2026-09-24-version-gated-gateway-migration.md)).
- Partial supersession: [API Gateway Usage Stats and Request Logs](../architecture/2026-09-17-ai-gateway-usage-logs.md) is retained and cross-linked; this record replaces only its unpriced eligibility, including the old claim that only a 2xx upstream response contributes tokens and cost — now corrected to non-streaming parsing from 2xx bodies while mid-stream failures keep their accumulated usage — and its SQLite logging, record-time price freeze, retention, `group_by` contract and `local_model` facet decisions remain in force.
