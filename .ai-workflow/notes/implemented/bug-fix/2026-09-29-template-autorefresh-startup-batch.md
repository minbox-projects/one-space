# Agent Note: Template Auto Refresh Runs One Batch at App Start

Status: implemented

English | [中文](2026-09-29-template-autorefresh-startup-batch.zh.md)

## Problem

The scheduler delivered by [Provider Templates Refresh Automatically on a Persisted Interval](../feature/2026-09-23-template-auto-refresh.md) only installed its timer when it read the persisted interval at app start. With the default 60-minute interval the first automatic sync therefore arrived a full interval after launch, and the timer is paused across system sleep, so after an app upgrade that changes the generated terminal model list (for example the per-model `provider.npm` override for OpenCode Responses-only models) or after a long sleep the tool-side terminal records could stay stale for hours until a tick finally ran. The observed incident: the opencode configuration was only regenerated at 08:26, when a template auto-refresh tick finally ran, hours after the 0.1.40 upgrade. Manual (non-template) edits are already surfaced by the `pending_sync` model-selection drift badge ([Gateway Terminal Sync Flags Model-Selection Drift as Pending](2026-09-29-gateway-terminal-pending-model-drift.md)), but the template-driven propagation had no startup promptness guarantee.

## Decision

`applyInterval(value, runOnce = false)` in `src/components/AiGateway/useTemplateAutoRefresh.ts` now runs exactly one immediate `runBatch()` after installing the interval schedule when `runOnce` is set and the normalized interval is a positive integer. The mount read (`readAndApplyInterval(true)`) passes `runOnce = true`; the `subscribeTemplateAutoRefreshIntervalChanged` notification re-reads and re-applies the schedule only and never starts an immediate batch. Interval `0`, invalid values and a failed interval read still mean no batch and no schedule. The existing `batchInFlightRef` guard dedupes the React StrictMode double mount, so the immediate batch runs exactly once.

The immediate run is the ordinary batch: it lists the templates, syncs every template with a non-blank `models_url` sequentially through the unchanged `ai_gateway_sync_provider_template`, defers templates with an in-flight manual sync, records failures in memory without toasts, and a successful sync still carries the best-effort terminal refresh ([Template Sync Best-Effort Refreshes Previously Synced Terminal Tools](../feature/2026-09-23-template-terminal-resync.md)) and the qualifying-change message ([Automatic Template Sync Notifies the Message Center on Real Mapping Changes](../feature/2026-09-24-ai-gateway-template-auto-sync-notification.md)).

## Alternatives considered

- Also running an immediate batch on every interval-change notification: declined because a Settings save is a schedule change rather than a data change; it would fetch every template on each save, while re-applying the schedule preserves the tick semantics without extra traffic.
- Keeping the schedule-only startup and relying on the `pending_sync` model-selection drift badge and manual syncs for the stale window: declined because the badge exists to expose manual (non-template) edits while template-driven changes are propagated automatically by design; the incident showed template-driven staleness lasting hours, so the schedule itself needed a startup batch.

## Consequences

- Startup: a valid persisted interval installs the schedule and then runs exactly one immediate batch; `0`, invalid values and a read failure run neither. The StrictMode double mount still produces one batch because the in-flight guard turns the second call into a no-op.
- Interval changes: a Settings save that notifies the seam re-reads the persisted interval and replaces the timer without an immediate batch; the next automatic run stays one full interval away (or none when the new value is `0`).
- Promptness: after an app upgrade or a long sleep, template-driven updates reach the derived providers and tool-side records at the first app start instead of waiting up to the interval; manual edits remain covered by the `pending_sync` badge.
- Unchanged batch semantics: URL-backed templates sync sequentially, manual-sync-in-flight templates are deferred, failures render inline without toasts, and successful syncs keep the best-effort terminal refresh and the qualifying-change message notification.
- Verification: `npx vitest run src/components/AiGateway/useTemplateAutoRefresh.test.ts` exit 0 (28 passed) covers `runsOneBatchImmediatelyAtMountThenOnlyOnTheIntervalAndSkipsBlankModelsUrl`, `anIntervalChangeNotificationNeverRunsAnImmediateBatch` and the re-based scheduler and notification `AC-*` tests; `npx vitest run src/components/AiGateway` exit 0 (390 passed, 16 files); `npx eslint src/components/AiGateway/useTemplateAutoRefresh.ts src/components/AiGateway/useTemplateAutoRefresh.test.ts` exit 0; `npx tsc -b --pretty false` exit 0.
- Supersession: partial. This record partially supersedes the startup lifecycle of [Provider Templates Refresh Automatically on a Persisted Interval](../feature/2026-09-23-template-auto-refresh.md): the mount read now also runs one immediate batch, while only the interval-change notification keeps the re-read-and-re-apply-without-batch behavior; that record's interval contract, batch semantics and failure-display decisions remain in force and are reused unchanged, and the record is corrected in place for the revised startup lifecycle. No other record is superseded.
- `MEMORY.md`, `docs/USAGE.md` and the `ai-gateway` entry of `.ai-workflow/index/navigation.json` record the startup-batch rule in the same change, and `navigation.md` is regenerated from the authoritative JSON.
