# Agent Note: Template Auto Refresh Runs One Batch at App Start

Status: implemented

English | [中文](2026-09-29-template-autorefresh-startup-batch.zh.md)

## Problem

The scheduler delivered by [Provider Templates Refresh Automatically on a Persisted Interval](../feature/2026-09-23-template-auto-refresh.md) only installed its timer when it read the persisted interval at app start. With the default 60-minute interval the first automatic sync therefore arrived a full interval after launch, and the timer is paused across system sleep, so after an app upgrade that changes the generated terminal model list or after a long sleep the tool-side terminal records could stay stale for hours until a tick finally ran. The observed incident: the opencode configuration was only regenerated hours after an upgrade, when a template auto-refresh tick finally ran. Manual (non-template) edits are already surfaced by the `pending_sync` model-selection drift badge, but the template-driven propagation had no startup promptness guarantee. [Core Workflows Cleanup and Optimization](../architecture/2026-10-09-core-workflows-cleanup-and-optimization.md) later moved the schedule into the Rust process and now owns this startup lifecycle; this record is retained for the startup-batch decision.

## Decision

The process-owned scheduler in `src-tauri/src/ai_gateway/auto_refresh.rs` installs exactly one startup batch. `start_scheduler` is called once from `src-tauri/src/app_runtime/run_app.rs` after gateway autostart; on install it reads the persisted interval and, when the normalized value is a positive integer, arms the schedule and then spawns exactly one immediate `run_batch`. A persisted `0` (and an invalid or failed interval read) installs the parked control without running a batch, so a later `request_rearm` can arm it without a batch. The install is guarded by a process-level slot, so a second setup call is a no-op and can never run a second startup batch. The interval-change path is separate: a persisted save calls `auto_refresh::request_rearm()`, which only replaces the deadline and never runs an immediate batch.

The immediate run is the ordinary batch: it lists the eligible templates and syncs every template with a non-blank `models_url` sequentially through the shared `execute_template_sync` path, skips a template another same-template operation is already refreshing, records failures in process memory without toasts, and a successful sync still carries the best-effort terminal refresh and the additions-only `template_sync` info message.

The frontend no longer owns any timer: `useTemplateAutoRefresh`, mounted in `src/App.tsx`, is a read/subscription adapter with one status read and one update-event subscription. The earlier frontend `batchInFlightRef` StrictMode dedupe and `applyInterval(value, runOnce)` helper no longer exist; the single process install replaces them.

## Alternatives considered

- Also running an immediate batch on every interval-change notification: declined because a Settings save is a schedule change rather than a data change; it would fetch every template on each save, while re-arming the schedule preserves the tick semantics without extra traffic.
- Keeping the schedule-only startup and relying on the `pending_sync` model-selection drift badge and manual syncs for the stale window: declined because the badge exists to expose manual (non-template) edits while template-driven changes are propagated automatically by design; the incident showed template-driven staleness lasting hours, so the schedule itself needed a startup batch.
- Keeping the startup batch in the frontend mount effect: declined because the process scheduler is the single owner after [Core Workflows Cleanup and Optimization](../architecture/2026-10-09-core-workflows-cleanup-and-optimization.md); a frontend batch would duplicate the schedule, run only while a WebView mounts and reintroduce the StrictMode dedupe the process install already handles.

## Consequences

- Startup: a valid persisted interval installs the schedule and then runs exactly one immediate batch; `0`, invalid values and a read failure run neither. The process-level install guard means a second setup call never runs a second batch.
- Interval changes: a Settings save re-arms the backend schedule through `ai_gateway_template_auto_refresh_save` / `request_rearm()` without an immediate batch; the next automatic run stays one full interval away (or none when the new value is `0`).
- Promptness: after an app upgrade or a long sleep, template-driven updates reach the derived providers and tool-side records at the first app start instead of waiting up to the interval; manual edits remain covered by the `pending_sync` badge.
- Unchanged batch semantics: URL-backed templates sync sequentially, same-template in-flight work is skipped or reused through the shared guard, failures render inline without toasts, and successful syncs keep the best-effort terminal refresh and the additions-only notification.
- Verification: `src-tauri/src/ai_gateway/tests/auto_refresh.rs` covers startup exactly once (positive interval runs one batch and a second install is a no-op), eligible-only selection, sequential order, tick skip and the re-arm path; `src/App.runtimeOwnership.test.tsx` covers the renderer performing one status read and one subscription and never listing or syncing templates at startup.
- Supersession: partial. [Core Workflows Cleanup and Optimization](../architecture/2026-10-09-core-workflows-cleanup-and-optimization.md) replaces this record's frontend mount-effect implementation of the startup batch with the process scheduler, while this record's startup-batch, no-interval-change-batch and promptness decisions remain in force; this record is retained and cross-linked, not archived. This record continues to partially supersede the startup lifecycle of [Provider Templates Refresh Automatically on a Persisted Interval](../feature/2026-09-23-template-auto-refresh.md), whose interval contract, batch semantics and failure-display decisions remain in force.
- `MEMORY.md` and the `ai-gateway` and `ai-gateway-backend` entries of `.ai-workflow/index/navigation.json` record the process startup-batch rule in the same change, and `navigation.md` is regenerated from the authoritative JSON.
