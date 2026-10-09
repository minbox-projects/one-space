# Agent Note: macOS Tray AI WorkFlow Submenu Shows Installed Agent Models

Status: implemented

English | [中文](2026-10-09-ai-workflow-tray-models.zh.md)

## Problem

The macOS tray menu exposed pages, services and shortcuts, but gave no view of the AI Workflow subagent model configuration that the AI Workflow Model Switcher manages, so a user had to open the app to see which model and reasoning effort each role would use. The saved active profile YAML is not a truthful source for that view: Save and Activate are separate actions, so the persisted profile definition can differ from the configuration the hosts have actually installed, and the tray must not present a saved definition as the current installed state.

## Decision

A read-only backend query and a tray submenu show the currently installed agent configuration rather than the saved profile definition.

- `src-tauri/src/ai_workflow_profiles.rs` adds the public function `get_active_models(home_override: Option<&Path>) -> Result<Option<ProfileMatrix>, String>` and registers the Tauri command `ai_workflow_get_active_models` in `src-tauri/src/app_runtime/run_app.rs`. It reads `~/.config/ai-workflow/config.yaml` for `active_profile` (absent or blank yields `None`) and then the fixed nine role files the three hosts install: Codex `.codex/agents/<role>.toml` with the `model` and `model_reasoning_effort` keys, Claude `.claude/agents/<role>.md` with `model` and `effort` YAML frontmatter, and OpenCode `.config/opencode/agents/<role>.md` with `model` and `reasoningEffort` frontmatter. It never reads a saved profile YAML and never runs the CLI. It performs no mutation; a missing host file yields that optional host as `None`, a present file with missing or blank fields yields empty strings, and an existing but malformed config or host file returns an error instead of stale data. It reuses the existing `ProfileMatrix`, `AgentMatrixRow` and `ModelEffort` types.
- `src/lib/aiWorkflowProfiles.ts` adds the public `getActiveModels` wrapper and exports `AI_WORKFLOW_PROFILE_UPDATED_EVENT = "ai-workflow-profile-updated"`; `activateProfile` dispatches that window event only after a successful activation.
- `src/lib/trayMenu.ts` extends `TrayMenuState` with an optional `aiWorkflow: { profile: ProfileMatrix | null; unavailable?: boolean }` and places the top-level AI WorkFlow item after `ai-usage` and before `more-pages`. It builds a disabled profile header, then nine role submenus, each with three disabled host rows showing the full model and reasoning effort, and falls back to bilingual loading, no-active-profile, unavailable and not-set text when the snapshot is absent, empty or failed.
- `src/App.tsx` refreshes the snapshot on mount, on the `main-window-visibility-changed` event, on `AI_WORKFLOW_PROFILE_UPDATED_EVENT` and on a 60-second interval that only contributes while the main window is hidden, and removes the interval and listener on cleanup. The latest-started request wins, a failed fetch clears the previous cells and marks the submenu unavailable, an identical snapshot avoids an extra native rebuild, and native menu application stays serialized through the existing apply chain.

## Alternatives considered

- Reading the saved active profile YAML under `~/.config/ai-workflow/profiles/<active>.yaml` for the tray: declined because Save and Activate are separate, so that saved definition can differ from the installed configuration and the tray would misreport the current state.
- Persisting a snapshot when a profile activates and having the tray read that snapshot: declined because activation already writes the installed files, which are the direct source, while a separate snapshot adds a write path and staleness for no read-only benefit.

## Consequences

- The tray reports the model and reasoning effort that newly created agents will use. It does not claim that running in-flight sessions hot reload, and it does not read current token usage.
- The query is strictly read-only: an absent or blank `active_profile` shows the no-active-profile message, and a corrupt existing config or installed file surfaces the unavailable message rather than stale cells.
- The `tray-menu` feature now depends on `ai-workflow-model-switcher` for the frontend wrapper and types, and `ai-workflow-backend` registers the added command as its ninth core command; `MEMORY.md` and the navigation index record both in the same change.
- This note does not supersede [Tray menu ownership and contract](../architecture/2026-09-22-tray-menu-ownership-and-contract.md): the left click toggles the window and the right click opens the native menu, and the [Save and Activate separation](2026-09-23-ai-workflow-profile-save-activation.md) stays the reason the installed files, not the saved YAML, are the source.
