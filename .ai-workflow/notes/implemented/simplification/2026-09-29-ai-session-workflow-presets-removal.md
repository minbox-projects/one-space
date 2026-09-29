# Agent Note: AI Terminal Session Workflow Presets and Runs Are Removed Full-Stack

Status: implemented

English | [中文](2026-09-29-ai-session-workflow-presets-removal.zh.md)

## Problem

The AI Terminal Sessions workflow presets and runs capability was removed to slim the app. It covered the presets editor, the dependency check and apply flow, the run history and replay, the quick-bar preset selector, the OmniSearch preset and run results, the Settings sync scope, the ten `workflows_*` commands, and the storage sync, migration and provider-id remap handling.

## Decision

The removal is full-stack with no flag or fallback path. The frontend surfaces and their call sites were deleted (`src/components/WorkflowPresetsPanel.tsx`, `src/components/RecentWorkflowRuns.tsx`, `src/lib/workflows.ts`), and the backend `workflows` module and its ten `workflows_*` commands were deleted. Local and shared sync, the local-data mirror, startup migration and provider-id remapping stop touching `workflow_presets.json` and `workflow_runs.json`. `SyncPolicy.workflow_presets` was removed from the type, its defaults and its serialization, while legacy `config.json` files stay loadable because unknown fields are ignored and the key is no longer written. Existing local and shared workflow data files are intentionally left untouched and inert. `src-tauri/src/runtime_profiles.rs` and the separate AI Workflow Model Switcher toolbox feature are retained.

## Alternatives considered

- Frontend-only removal keeping the backend commands and storage: declined because it would leave unreachable code and keep the persistent sync, migration and remap handling alive for a feature with no entry point.
- Also removing the toolbox AI Workflow Model Switcher: declined because it is a separate feature outside the AI Terminal Sessions scope, with its own profile storage and commands.
- Deleting the existing workflow data files: declined as destructive and unnecessary because the files are inert without the feature, and keeping them leaves an older build able to read its data.

## Consequences

- No workflow entry point remains in the AI Terminal Sessions page, the quick bar, OmniSearch or the settings sync scope; the removed `workflows_*` commands fail as unknown commands, so a stale frontend build cannot call them and receives the error instead of a crash.
- Existing `workflow_presets.json` and `workflow_runs.json` files and their shared-profile copies remain on disk byte-identical, and no new workflow shared copy is created.
- The settings sync scope no longer offers the removed scope; a legacy `config.json` whose `sync_policy` still carries `workflow_presets` loads with every other scope value preserved, and the next save serializes a policy without the key.
- `materialize_strict_profile` and `cleanup_stale_runtime_profiles` in `src-tauri/src/runtime_profiles.rs` have no remaining callers and produce dead-code warnings, but are kept because session launch still uses `runtime_env_for_profile` for existing sessions.
- `data/runtime_profiles/` artifacts remain on disk and inert.
- No active note is fully or partially superseded: the notes tree contains no record of the removed workflow presets and runs capability, and the [AI Workflow Model Switcher record](../feature/2026-09-22-ai-workflow-model-switcher.md) and [AI Workflow Profile Save and Activation record](../feature/2026-09-23-ai-workflow-profile-save-activation.md) remain current because that separate feature is retained.
