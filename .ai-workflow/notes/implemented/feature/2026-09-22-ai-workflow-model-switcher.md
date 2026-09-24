# Agent Note: AI Workflow Model Switcher Delivers 9-by-3 Matrix with Backend Profile Commands

Status: implemented

English | [中文](2026-09-22-ai-workflow-model-switcher.zh.md)

## Problem

Switching and configuring AI Workflow subagent profiles previously required manual file editing of YAML files under `~/.config/ai-workflow/profiles/` or invoking the `ai-workflow profile activate <name>` CLI without visibility into model allocations across hosts. Developers managing multiple agents across Codex, Claude, and OpenCode hosts could not inspect role-by-host model configurations in a unified view, lacked model autocompletion tailored to each host's local configuration, and had no safe batch-update mechanism. Directly editing profile YAMLs bypassed schema validation and carried risks of corrupting configuration or failing activation when managed subagent files drifted.

## Decision

OneSpace delivers the standalone toolbox tool `ai-workflow-model-switcher` backed by dedicated backend Tauri commands and a 9-by-3 role-by-host editing matrix.

The implementation is split into a frontend toolbox matrix and a safe backend runtime:

1. Frontend Toolbox Matrix: Built in `src/components/AiWorkflowModelSwitcher/` and wired across six points (`src/lib/navigation.ts`, `src/lib/moreToolPresentation.ts`, `src/lib/launcherToolVisibility.ts`, `src/components/MoreToolsHub.tsx`, `src/components/Launcher.tsx`, and `src/App.tsx`). It provides a profile selector with active status, a 9-by-3 grid mapping 9 schema subagent roles (`backend`, `documentation-maintainer`, `file-explorer`, `frontend`, `git-operator`, `researcher`, `spec-review`, `standards-review`, `test`) against 3 hosts (`codex`, `claude`, `opencode`), model source dropdowns with manual input fallback, reasoning effort selectors (limited to the six enum values `low`, `medium`, `high`, `xhigh`, `max`, `ultra`), column/row/all batch-apply actions, dirty state tracking, a Save action that uses `ai_workflow_save_profile` without activating an existing profile, and a separate Activate action that calls `activateProfile` for saved YAML. Profiles created in this UI receive one Yes/No confirmation after their first successful Save: No leaves the profile saved without activation; Yes separately activates the persisted profile. A failed Save does not prompt, activation failure does not discard the saved matrix, and a successful save updates the dirty baseline.
2. Backend Profile Runtime: Implemented in `src-tauri/src/ai_workflow_profiles.rs` and `src-tauri/src/ai_workflow_profiles/`, registering eight Tauri commands (`ai_workflow_list_profiles`, `ai_workflow_get_profile_matrix`, `ai_workflow_get_model_sources`, `ai_workflow_activate_profile`, `ai_workflow_save_profile`, `ai_workflow_save_and_activate_profile`, `ai_workflow_create_profile`, `ai_workflow_delete_profile`). The backend aggregates host model candidates from local configurations (`opencode.json` provider models, `config.toml` top-level and profile models, and `settings.json` env keys), isolates single-source failures to allow other columns to function, performs strict schema validation (`version: 1.0.0`, paired model and effort values, and sanitized profile names), and atomically saves YAML. The save-only `ai_workflow_save_profile` path neither invokes the CLI nor mutates active-profile state. The save-and-activate path snapshots target YAML bytes before writing, resolves the `ai-workflow` binary to trigger activation, and automatically restores the snapshot on activation failure.

## Alternatives considered

- Pure CLI alternative (adding interactive `profile edit` or interactive matrix prompts directly in `ai-workflow` CLI): declined because OneSpace is the unified desktop workbench where developers manage AI environments, gateways, and terminal sessions; a 9-by-3 matrix spanning 27 cells and multiple batch operations is cumbersome in terminal prompts, and a graphical toolbox tool provides immediate visual comparison across hosts.
- Frontend directly reading and modifying `~/.config/ai-workflow/profiles/` YAML files: declined because OneSpace maintains strict separation between frontend presentation and filesystem mutations; keeping YAML validation, atomic writes, CLI execution, and snapshot rollback in the Tauri backend guarantees safety and prevents inconsistent dirty files.
- Live `/v1/models` network fetching from remote providers: declined because the switcher operates on locally configured and supported models across local tool configs (`opencode.json`, `config.toml`, `settings.json`); relying on network calls would introduce latency, network flakiness, and credential exposure, whereas local inspection guarantees offline availability.

## Consequences

- The backend serves as the single authority for profile YAML mutations and profile activation commands; the frontend interacts solely through typed Tauri invocations in `src/lib/aiWorkflowProfiles.ts` without directly accessing configuration paths. Save-only persistence is separate from activation, and saving an existing profile cannot implicitly change the active profile.
- Model sources gracefully isolate failures: an unreadable Codex or OpenCode configuration only marks that specific column with an actionable error and enables manual input fallback, leaving other host columns completely functional.
- Save-only YAML writes are atomic and do not run the CLI or alter active-profile state. For save-and-activate, snapshot rollback guarantees that activation failures (such as CLI missing or modified managed agent files) restore previous profile bytes verbatim, preserving error messages from the CLI without leaving dirty configurations.
- API keys and sensitive credentials are never read, logged, or serialized into profile YAMLs.
- Navigation index registers the `ai-workflow-model-switcher` feature in `.ai-workflow/index/navigation.json` under module root `frontend` with owner `frontend`, `navigation.md` is regenerated, and `MEMORY.md` documents the matrix editing and rollback standards.
- The rationale for making existing-profile Save persistence-only and prompting only after a UI-created profile's first successful save is recorded in [AI Workflow Profile Save and Activation Are Separate Actions](2026-09-23-ai-workflow-profile-save-activation.md); this note retains the broader switcher decision and alternatives.

## Decision

OneSpace delivers the standalone toolbox tool `ai-workflow-model-switcher` backed by dedicated backend Tauri commands and a 9-by-3 role-by-host editing matrix.

The implementation is split into a frontend toolbox matrix and a safe backend runtime:

1. Frontend Toolbox Matrix: Built in `src/components/AiWorkflowModelSwitcher/` and wired across six points (`src/lib/navigation.ts`, `src/lib/moreToolPresentation.ts`, `src/lib/launcherToolVisibility.ts`, `src/components/MoreToolsHub.tsx`, `src/components/Launcher.tsx`, and `src/App.tsx`). It provides a profile selector with active status, a 9-by-3 grid mapping 9 schema subagent roles (`backend`, `documentation-maintainer`, `file-explorer`, `frontend`, `git-operator`, `researcher`, `spec-review`, `standards-review`, `test`) against 3 hosts (`codex`, `claude`, `opencode`), model source dropdowns with manual input fallback, reasoning effort selectors (limited to the six enum values `low`, `medium`, `high`, `xhigh`, `max`, `ultra`), column/row/all batch-apply actions, dirty state tracking, direct activation, and save-and-activate actions with detailed activation reports.
2. Backend Profile Runtime: Implemented in `src-tauri/src/ai_workflow_profiles.rs` and `src-tauri/src/ai_workflow_profiles/`, registering five Tauri commands (`ai_workflow_list_profiles`, `ai_workflow_get_profile_matrix`, `ai_workflow_get_model_sources`, `ai_workflow_activate_profile`, `ai_workflow_save_and_activate_profile`). The backend aggregates host model candidates from local configurations (`opencode.json` provider models, `config.toml` top-level and profile models, and `settings.json` env keys), isolates single-source failures to allow other columns to function, performs strict schema validation (`version: 1.0.0`, paired model and effort values, and sanitized profile names), creates pre-write byte snapshots before saving, resolves the `ai-workflow` binary to trigger activation, and automatically rolls back the YAML on activation failures to prevent corrupt state.

## Alternatives considered

- Pure CLI alternative (adding interactive `profile edit` or interactive matrix prompts directly in `ai-workflow` CLI): declined because OneSpace is the unified desktop workbench where developers manage AI environments, gateways, and terminal sessions; a 9-by-3 matrix spanning 27 cells and multiple batch operations is cumbersome in terminal prompts, and a graphical toolbox tool provides immediate visual comparison across hosts.
- Frontend directly reading and modifying `~/.config/ai-workflow/profiles/` YAML files: declined because OneSpace maintains strict separation between frontend presentation and filesystem mutations; keeping YAML validation, atomic writes, CLI execution, and snapshot rollback in the Tauri backend guarantees safety and prevents inconsistent dirty files.
- Live `/v1/models` network fetching from remote providers: declined because the switcher operates on locally configured and supported models across local tool configs (`opencode.json`, `config.toml`, `settings.json`); relying on network calls would introduce latency, network flakiness, and credential exposure, whereas local inspection guarantees offline availability.

## Consequences

- The backend serves as the single authority for profile YAML mutations and profile activation commands; the frontend interacts solely through typed Tauri invocations in `src/lib/aiWorkflowProfiles.ts` without directly accessing configuration paths.
- Model sources gracefully isolate failures: an unreadable Codex or OpenCode configuration only marks that specific column with an actionable error and enables manual input fallback, leaving other host columns completely functional.
- Atomic saving and snapshot rollback guarantee that activation failures (such as CLI missing or modified managed agent files) restore previous profile bytes verbatim, preserving error messages from the CLI without leaving unsaved or dirty configurations.
- API keys and sensitive credentials are never read, logged, or serialized into profile YAMLs.
- Navigation index registers the `ai-workflow-model-switcher` feature in `.ai-workflow/index/navigation.json` under module root `frontend` with owner `frontend`, `navigation.md` is regenerated, and `MEMORY.md` documents the matrix editing and rollback standards.
