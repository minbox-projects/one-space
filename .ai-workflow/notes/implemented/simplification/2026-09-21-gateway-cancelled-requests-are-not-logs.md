# Agent Note: Gateway Cancelled Requests Are Not Logs

Status: implemented

English | [中文](2026-09-21-gateway-cancelled-requests-are-not-logs.zh.md)

## Problem

The gateway treated a downstream tool connection closing, or a completed upstream response becoming undeliverable, as a stored business outcome. It wrote a synthetic `cancelled` terminal row and retained any upstream attempts that had already completed. This made an internal transport lifecycle event visible as a request-log status even though the inbound OpenCode or Codex request did not complete for its caller. Historical `cancelled` rows also affected request-log pages, facets, groups and timestamps, so removing only the visible filter would not have removed the state consistently.

## Decision

A downstream TCP close or response-delivery failure is not a business request-log outcome. The connection handler discards that inbound request's entire buffered usage-log set, including every upstream attempt completed before cancellation, and writes no synthetic `cancelled` terminal row. It still cancels unfinished forwarding, retry waits and later attempts promptly; provider health, auto-disable, session affinity and caller-visible response behavior are unchanged. Normally completed success, failure, recovered multi-attempt and no-candidate requests keep the existing per-attempt rows and exactly one terminal row, including the synthetic HTTP 502 terminal row for no candidates.

Existing rows whose stored `result` is `cancelled` remain physically present and readable by raw or compatibility code. Every user-visible request-log query excludes them consistently from ungrouped records, `total`, `total_pages`, page clamping, the `models` facet, model/day groups, request and error counts and last-request timestamps. A compatibility `status="cancelled"` filter remains valid and returns an empty page. The visible frontend filter offers only success and failure and defensively removes a stale cancelled record before rendering. Rust `UsageResult::Cancelled` parsing and serialization, the TypeScript compatibility union member and the translation helper remain available; no schema migration, row rewrite or historical deletion is performed.

## Alternatives considered

- Keep the synthetic terminal row but hide only its frontend badge: declined because backend totals, pagination, facets, grouping and timestamps would still expose cancelled transport events and stale clients could still render them.
- Keep completed attempts while dropping only the synthetic terminal row: declined because those attempts belong to the same inbound request that never completed as a business outcome; retaining them would make a cancelled request partially visible and would preserve upstream cost attribution only by violating the selected request-log boundary.
- Delete or migrate historical cancelled rows: declined because compatibility does not require destructive data rewriting, existing retention already governs physical cleanup, and keeping parsers and types allows old databases and stale payloads to remain readable.
- Remove the cancelled enum/type/filter compatibility surface: declined because historical databases and stale callers can still supply that value; accepting it as an empty result avoids a breaking input change without making it visible.

## Consequences

- A cancelled or undeliverable inbound request writes zero rows even when one or more attempts, including an upstream success, completed before the disconnect. The approved trade-off is loss of request-log visibility for that completed upstream work and any cost it incurred.
- Historical `cancelled` rows remain in `api_gateway_usage.db` until ordinary retention removes them, but all user-visible request-log pages, totals, pages, model facets, model/day groups and timestamps exclude them. A legacy cancelled status filter succeeds with an empty page, and the interface offers and renders no cancelled state.
- Compatibility representations remain in Rust and TypeScript, so this simplification needs no SQLite schema migration, data deletion or payload-shape change.
- Normal success, failure, recovered multi-attempt and no-candidate logging semantics remain unchanged, as do error text, token and cost handling, retention and attempt labels.
- Partial supersession: [Gateway Per-Attempt Request Logging and Stored Error Text](../architecture/2026-09-20-gateway-per-attempt-logging-and-error-text.md) remains authoritative for normally completed per-attempt logging, terminal rows, stored error text, migration defaults and privacy boundaries. This record replaces only its cancellation decision that retained completed attempts, wrote a synthetic `cancelled` terminal row and exposed historical cancelled rows in request-log views.
