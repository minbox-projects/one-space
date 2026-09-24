# Agent Note: Gateway Cancelled Requests Are Not Logs

Status: implemented

English | [中文](2026-09-21-gateway-cancelled-requests-are-not-logs.zh.md)

## Problem

The gateway treated a downstream tool connection closing, or a completed upstream response becoming undeliverable, as a stored business outcome. It wrote a synthetic `cancelled` terminal row and retained any upstream attempts that had already completed. This made an internal transport lifecycle event visible as a request-log status even though the inbound OpenCode or Codex request did not complete for its caller. Historical `cancelled` rows also affected request-log pages, facets, groups and timestamps, so removing only the visible filter would not have removed the state consistently.

## Decision

A downstream TCP close or response-delivery failure is not a business request-log outcome. The connection handler discards that inbound request's entire buffered usage-log set, including every upstream attempt completed before cancellation, and writes no synthetic `cancelled` terminal row. It still cancels unfinished forwarding, retry waits and later attempts promptly; provider health, auto-disable, session affinity and caller-visible response behavior are unchanged. Normally completed success, failure, recovered multi-attempt and no-candidate requests keep the existing per-attempt rows and exactly one terminal row, including the synthetic HTTP 502 terminal row for no candidates.

Existing rows whose stored `result` is `cancelled` do not survive the upgrade: the first open of an older usage database deletes every `cancelled` row transactionally and exactly once under a database-level version gate (`PRAGMA user_version` advancing to 1), and later opens delete nothing. Every user-visible request-log query already excluded those rows from ungrouped records, `total`, `total_pages`, page clamping, the `models` facet, model/day groups, request and error counts and last-request timestamps, so the deletion changes no visible page, total or group. Afterwards the backend neither defines, parses nor filters `cancelled`, the frontend neither declares, filters, styles nor labels it, and no compatibility `status="cancelled"` alias remains ([Gateway Migration Is Permanent and Version-Gated](../architecture/2026-09-24-version-gated-gateway-migration.md)).

## Alternatives considered

- Keep the synthetic terminal row but hide only its frontend badge: declined because backend totals, pagination, facets, grouping and timestamps would still expose cancelled transport events and stale clients could still render them.
- Keep completed attempts while dropping only the synthetic terminal row: declined because those attempts belong to the same inbound request that never completed as a business outcome; retaining them would make a cancelled request partially visible and would preserve upstream cost attribution only by violating the selected request-log boundary.
- Keep historical `cancelled` rows and their compatibility surface forever: declined because the state was already invisible to every user-visible query, so keeping it only preserved a permanent exclusion and a dead parser and filter surface; deleting the rows once under a database version gate removes the state without changing any visible result.
- Delete `cancelled` rows on every open, or rewrite them into a visible status: declined because deletion must be provably once and must never reclassify a transport event as a business outcome; the version gate records that the cleanup completed, and reclassification would fabricate a log row the request never produced.

## Consequences

- A cancelled or undeliverable inbound request writes zero rows even when one or more attempts, including an upstream success, completed before the disconnect. The approved trade-off is loss of request-log visibility for that completed upstream work and any cost it incurred.
- Historical `cancelled` rows are physically deleted from `api_gateway_usage.db` when an older database is first opened, exactly once under the database version gate; because every user-visible request-log page, total, page, model facet, model/day group and timestamp already excluded them, no visible result changes, and the state cannot be restored.
- The Rust `UsageResult` member with its parser and SQL exclusion, the TypeScript compatibility member, the translation helper, the defensive frontend filter and the compatibility `status="cancelled"` input are removed; the cleanup advances the database version instead of rewriting remaining rows, and no payload shape changes.
- Normal success, failure, recovered multi-attempt and no-candidate logging semantics remain unchanged, as do error text, token and cost handling, retention and attempt labels.
- Partial supersession: [Gateway Per-Attempt Request Logging and Stored Error Text](../architecture/2026-09-20-gateway-per-attempt-logging-and-error-text.md) remains authoritative for normally completed per-attempt logging, terminal rows, stored error text, migration defaults and privacy boundaries. This record replaces only its cancellation decision that retained completed attempts, wrote a synthetic `cancelled` terminal row and exposed historical cancelled rows in request-log views.
- Partial supersession: [Gateway Migration Is Permanent and Version-Gated](../architecture/2026-09-24-version-gated-gateway-migration.md) replaces only this record's decision to keep historical `cancelled` rows physically present and to retain the compatibility representation; the rule that a cancelled or undeliverable inbound request writes zero rows, and every other decision here, stand.
