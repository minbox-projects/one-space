# Agent Note: AI Workspace Removed End to End

Status: implemented

English | [中文](2026-09-29-ai-workspace-removal.zh.md)

## Problem

OneSpace carried an in-app AI chat workspace beside the native-terminal AI Sessions: the `AiWorkspace` and `SmartWorkspaceHub` surfaces, a model center, assistant connection settings, a `Schedules` page, Quick and Selection assistant floating windows, and the `aiWorkspace`/`aiAssistant`/`assistantToolCalls`/`assistantMcpDisplay` wrappers. Its backend was the `src-tauri/src/ai_assistant` module (commands, conversations, model request, providers, scheduler, schedules, settings, state, tools, types, tests) and the `assistant_mcp` bridge, exposing the `ai_workspace_bootstrap`, `workspace_settings_*`, `workspace_model_roles_*`, `workspace_assistant_*`, `workspace_conversation_*`, `workspace_automation_*`, `workspace_schedule_*`, `workspace_quick_assistant_*` and `workspace_selection_assistant_*` command families, `provider_connection_test`, `provider_models_fetch`, the `assistant_mcp::*` commands and the assistant window commands. The feature duplicated the AI Sessions terminal flow, kept its own per-profile files (`ai_workspace_state.json` and `data/mcp/assistant_mcp_tool_previews.json`), and its capability surface had already been narrowed when `notes_search` was withdrawn in the [Toolbox Plugin Registry Replaces Hand-Maintained Tool Lists](../architecture/2026-09-25-toolbox-plugin-registry.md) change.

## Decision

The whole workspace was deleted instead of hidden, feature-flagged or reduced to its backend.

- The frontend surfaces and wrappers are gone, together with their tray, sidebar, Launcher, OmniSearch, Settings, in-app Documentation and i18n entries. The Quick AI Session Bar and every `quick-ai` entry point remain.
- The backend `ai_assistant` tree and `assistant_mcp.rs` are gone, and the invoke handler no longer registers the workspace commands, the provider test and fetch commands, the `assistant_mcp::*` commands, the scheduler initialization or the assistant window commands. The command surface is a public interface, so this is a deliberate public-interface change: an older frontend build receives the standard unknown-command error instead of crashing.
- Startup performs a one-shot best-effort cleanup. `app_runtime::run_app::run()` calls `cleanup_removed_ai_workspace_files()` right after `config::seed_dev_gateway_files_on_start()`; the wrapper resolves the current profile's local data directory through `crate::get_data_dir()` and delegates to `cleanup_removed_ai_workspace_files_in(base)`, which calls `fs::remove_file` for exactly `ai_workspace_state.json` and `data/mcp/assistant_mcp_tool_previews.json`. Missing files are ignored, other failures are logged and retried on the next start, startup is never blocked, and no other path is touched.
- Shared state is preserved: the MCP server list (including the seeded `mcp-exa` and `mcp-context7` servers), the message center, the MCP Servers page and the retained `mcp_runtime` and `mcp_templates` modules were not changed by the cleanup.

## Alternatives considered

- Keep the assistant backend and delete only the UI: declined because the commands, scheduler, providers and storage would remain a maintained but unreachable surface with no production consumer, and every future build, test and review would still carry it.
- Hide the entry points or park the feature behind a flag: declined because hiding removes no code, command, scheduler or per-profile file, keeps the public command surface alive and leaves a dormant feature that still owes maintenance and compatibility.
- Delete the seeded MCP servers (`mcp-exa`/`mcp-context7`) together with the assistant preview cache: declined because the MCP server list is shared with the retained MCP Servers page and terminal projections, so deleting user-visible shared data would change behavior beyond the removed feature.
- Leave the two feature-owned files on disk instead of cleaning them at startup: declined because nothing reads them anymore and they would persist as dead per-profile state; the approved cost is that the cleaned files cannot be recovered.

## Consequences

- No workspace, assistant, scheduler or floating assistant window remains: no sidebar, tray, Launcher, OmniSearch, Settings or in-app documentation entry point, and no navigation feature for it. Documentation (`docs/USAGE.md`, README) and the roadmap document were updated in the same change, and `MEMORY.md` records the cleanup convention.
- An older frontend build that still calls the removed commands receives the standard unknown-command error rather than a crash; because the cleaned files cannot be recovered, that build's assistant workspace starts empty.
- The cleanup is idempotent and bounded: exactly the two fixed files below the current profile's local data directory are removed, an unrelated `data/mcp` file is preserved (asserted by the new Rust test), and the shared MCP list and message center are untouched.
- After the assistant bridge was deleted, `mcp_runtime` and `mcp_templates::find_mcp_template_for_server` have no production callers in the tree; both are retained untouched, and `mcp_templates` still serves the MCP Servers page through its `list_mcp_templates` and `get_mcp_template` commands. The scheduler had been the only dependent of the `chrono-tz` dependency, so it left `Cargo.toml`/`Cargo.lock`.
- The assistant capability contract no longer exists: `AgentToolPolicy`, the capability snapshot, the model tool definitions, default agents, conversation prompts, the dispatcher and every assistant i18n key are gone, while `quick_ai_shortcut` and the Bot icon used by subagents stay.
- Partial supersession: [Toolbox Plugin Registry Replaces Hand-Maintained Tool Lists](../architecture/2026-09-25-toolbox-plugin-registry.md) remains authoritative for the registry decision and its other removals; this record replaces only its references to a live assistant capability surface and to smart-workspace navigation aliases (the capability badges and toggles, and the smart-workspace alias resolution). Both records are retained and cross-linked.
