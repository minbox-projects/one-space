# Agent Note: AI Gateway Provider Template Reset Is Removed Full-Stack

Status: implemented

English | [中文](2026-10-08-provider-template-reset-removal.zh.md)

## Problem

The AI Gateway provider templates' "Restore built-in presets" capability was removed to slim the feature surface. The action let an operator clear every `deleted_template_ids` tombstone and bring all built-in templates back at once, but none of the remaining template actions depended on it, and its only purpose — undoing a built-in template deletion — is better served by leaving the deletion final. The capability was removed end to end: both UI toolbar entries, the frontend command wrapper, the Tauri command, the backend implementation, the `lib.rs` export, the `generate_handler!` registration, the related tests and the two bilingual i18n keys.

## Decision

The removal is full-stack with no feature flag and no fallback path. On the frontend, `ProviderTemplateSection` lost its `onResetBuiltin` prop and its restore button, and `src/components/AiGateway/index.tsx` lost the dialog-toolbar restore button and its `handleResetBuiltinTemplates` handler; the now-unused `RotateCcw` imports, the `template-section-reset-btn` testid and the `aiGatewayResetProviderTemplates` wrapper in `src/lib/aiGateway.ts` were deleted together. On the backend, `ai_gateway_reset_provider_templates` was removed from the command module, the `apply_reset_provider_templates` implementation was removed from `src-tauri/src/ai_gateway/templates.rs`, and the command was dropped from the `lib.rs` export and the `generate_handler!` registration in `src-tauri/src/app_runtime/run_app.rs`. The tests were updated to pin the absence: `AiGateway.test.tsx` now asserts `template-section-reset-btn` is absent while the new-template control stays; `src-tauri/src/ai_gateway/tests.rs` asserts the command is registered in neither the registrar source nor the `lib.rs` export source; and the reset half of `test_template_delete_succeeds_when_unused_and_reset_restores` was dropped, leaving `test_template_delete_succeeds_when_unused` with its tombstone and hidden-view assertions. The bilingual keys `aiGatewayTemplateResetBuiltin` and `aiGatewayTemplateResetSuccess` were removed from both the English and Chinese sides of `src/i18n.ts`.

The retained `deleted_template_ids` tombstone semantics are unchanged. Deleting a built-in template still writes its id into `deleted_template_ids` exactly once and hides it from `provider_template_views`, and `effective_template` still fails for a deleted id; the removed reset command was the only path that cleared tombstones to bring a deleted built-in back, so after this removal no reset path clears them. The per-id `retain` inside `apply_upsert_provider_template`, which drops only the id of a template being explicitly re-saved, is unrelated to the removed capability, is out of scope and stays unchanged.

## Alternatives considered

- Remove only the UI entry points while keeping `ai_gateway_reset_provider_templates` and `apply_reset_provider_templates`: declined because it would leave an unreachable but callable command that still clears tombstones, so a stale frontend build could resurrect deleted built-in templates and the command would stay exported and registered with no remaining caller.
- Clear `deleted_template_ids` once in a migration: declined because it would rewrite persisted user intent by resurrecting templates the operator had deliberately deleted, and no migration or any persisted-format change is in scope for this removal.
- Keep the command behind a feature flag: declined because the capability has no remaining consumer, and a flag would keep the reset path, its registration and its config surface alive with no requested rollback or compatibility requirement.

## Consequences

- No restore entry point remains: the provider-template dialog toolbar renders only expand/collapse and new-template controls, and the embedded section renders no reset action, so deleting a built-in template from the app is now irreversible.
- Existing tombstones stay untouched: `deleted_template_ids` and `provider_templates` are not rewritten, so a previously deleted built-in stays hidden exactly as persisted and the persisted format is unchanged.
- A stale frontend build that still calls `ai_gateway_reset_provider_templates` receives an unknown-command error, writes nothing and does not crash, matching the project's prior removals.
- No migration and no persisted-format change: an existing `ai_gateway.json` loads and behaves identically apart from the missing reset action.
- No active note is fully or partially superseded: [API Gateway Provider Templates and Incremental Model Sync](../architecture/2026-09-18-api-gateway-provider-templates.md) and [Provider Templates Drop Built-in Model Catalogs and Prices](../architecture/2026-09-19-provider-template-manual-model-sync.md) document the template binding, sync, deletion and ignored-model lifecycle, but neither documents the reset capability, so both records remain current.
