# Agent Note: Automatic Template Sync Notifies the Message Center on Real Mapping Changes

Status: implemented

English | [中文](2026-09-24-ai-gateway-template-auto-sync-notification.zh.md)

## Problem

The [automatic template refresh](2026-09-23-template-auto-refresh.md) synced every URL-backed provider template on the persisted interval and propagated the result to the providers bound to it, but it did so silently. An operator could not tell when an upstream catalog had actually gained or retired models for a bound provider short of opening the template and reading the mapping list, so a notification was needed — with a deliberately narrow trigger. It had to fire only on real mapping changes and never for field-only differences, for the manual "sync model list" action, or for failed refreshes. It also could not become a second sync implementation, and it could not expand the backend command surface or the message schema.

## Decision

The scheduler in `src/components/AiGateway/useTemplateAutoRefresh.ts` reads `aiGatewayGetConfig()` once before the per-template loop as a running baseline, re-reads it after each successful `aiGatewaySyncProviderTemplate`, and diffs the `template_id`-matched providers by `upstream_model`. `computeTemplateSyncChange(previous, current, templateId)` treats a mapping whose `upstream_model` was absent from the previous configuration as an addition, and a mapping whose `enabled` transitioned from true or absent to an explicit `false` as a disable. Field-only differences (display name, effective protocol, provider name, `base_url`), mappings already disabled before the sync, models skipped by the template or listed in `ignored_models`, providers not bound to the template, and templates with no bound provider all do not qualify, so a template with no qualifying change produces nothing.

A qualifying template produces exactly one message payload built by `buildTemplateSyncMessage(view, change)`: `{ source: "ai_gateway", category: "template_sync", severity: "info", target: { tab: "ai-gateway" } }` with no `dedupe_key`. Its title names the template through `aiGatewayTemplateSyncNotificationTitle`; its summary is the `"; "`-joined non-empty parts of the affected-provider count plus the added count and the disabled count, with each zero count clause omitted (`aiGatewayTemplateSyncNotificationProviderCount`, `...AddedCount`, `...DisabledCount`); its detail is a newline-joined per-provider line (`aiGatewayTemplateSyncNotificationDetailProvider`) listing each affected mapping's `local_model`, falling back to `upstream_model` when blank.

`src/i18n.ts` ships all six keys in both languages (`aiGatewayTemplateSyncNotificationTitle`, `...ProviderCount`, `...AddedCount`, `...DisabledCount`, `...DetailProvider` and `messageSource_ai_gateway`), and `src/components/MessageCenter.tsx` adds the `ai_gateway` source label resolved through `messageSource_ai_gateway`.

Failure isolation is explicit. A failure to read the configuration taken before the batch suppresses every notification in that batch while every syncable template is still synced; a failure to read the configuration after an individual sync suppresses only that template's message and leaves the running baseline untouched for the next template; a failure while creating the message suppresses only that template's message. Neither failure blocks or rolls back a sibling sync. The manual "sync model list" path is untouched and creates no message.

## Alternatives considered

- Notifying on every successful automatic sync: declined because an unchanged catalog would produce a message on every tick, so the operator could not distinguish a real catalog change from routine churn.
- Adding a per-template `dedupe_key`: declined because repeated real changes are independent events worth their own record; a dedupe key would silently merge a later addition into an earlier message and hide the second change.
- Generating the notifications in the Rust backend: declined because the batch loop, the in-flight-manual-sync registry and the failure store already live in the frontend scheduler, so a backend notification would add a second execution path and a new emission point for no behavioral gain.
- Also notifying the manual "sync model list" action: declined because that action is operator-initiated and already reports its result synchronously, so a message would duplicate feedback the operator just requested.
- Detecting changes by comparing whole mapping objects rather than `upstream_model` presence plus a true-to-false `enabled` transition: declined because field-only propagation (display name, effective protocol) would then be misreported as a model change.

## Consequences

- Exactly one aggregated message per changed template per sync: the affected-provider count aggregates every bound provider of that template that qualified, and one batch creates one message per changed template.
- Independent messages for repeated changes: no `dedupe_key` is set, so two consecutive qualifying cycles create two independent messages; this is deliberate and accepted.
- Manual sync and automatic failures never notify: the manual path does not call the notification helper, and a failed sync records its inline failure reason without creating a message.
- Message text follows the active language: the six keys exist in both bundles and the title, summary and detail are resolved at creation time.
- No backend command, persistent-format or message-schema change: the notification reuses the existing `messages_create` command and only reads the existing encrypted gateway configuration.
- Verification: the delivered behavior tests are `src/components/AiGateway/useTemplateAutoRefresh.test.ts` for the added, disabled and non-qualifying diff, aggregation, envelope, no-dedupe repetition, failure isolation and bilingual rendering; `src/components/AiGateway/AiGateway.test.tsx` for the manual path creating no message; and `src/i18n.test.ts` for the new keys existing in both bundles.
- Relationship: this record extends [Provider Templates Refresh Automatically on a Persisted Interval](2026-09-23-template-auto-refresh.md), whose interval, scheduler and failure-display decisions remain in force and are reused unchanged; no supersession is claimed and no existing record is changed. It does not supersede [Template Sync Best-Effort Refreshes Previously Synced Terminal Tools](2026-09-23-template-terminal-resync.md): the notification observes the sync result read-only and leaves the shared command's best-effort terminal refresh unchanged.
- `MEMORY.md` records the qualifying-message and no-notification-on-manual-or-failure behavior in the `模板自动刷新` bullet in the same change; the navigation JSON and its generated Markdown need no change because the new helpers are module-private and no public symbol, path or ownership changed.
