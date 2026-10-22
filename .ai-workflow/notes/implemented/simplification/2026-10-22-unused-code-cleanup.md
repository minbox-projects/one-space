# Agent Note: Unused Code Audit Cleanup Retires Dormant MCP Runtime and Interfaces

Status: implemented

English | [中文](2026-10-22-unused-code-cleanup.zh.md)

## Problem

The project carried audited code with no production consumer: a dormant MCP session runtime and template matcher, registered IPC wrappers and their exclusive implementation chains, unreachable backup and model-pool UI actions with ignored props, orphan modules, types, CSS, dependencies, assets and prompt files, and unread bilingual keys. These surfaces still cost build time, maintenance and review attention even though no reachable workflow depended on them. Earlier project records also described some of the retired code as deliberately retained, so the recorded standards no longer matched the tree.

## Decision

The audit removed the proven orphans instead of hiding them behind flags or leaving dormant compatibility surfaces.

- Retire the dormant MCP session runtime (`mcp_runtime`) and the `mcp_templates::find_mcp_template_for_server` / `server_matches_template` matcher together with their exclusive tests. Keep the MCP template catalog (`get_mcp_templates`) and its list/get commands (`list_mcp_templates`, `get_mcp_template`), managed servers, messages and terminal projections.
- Withdraw the audited unused IPC names and their exclusive helpers, including the backup-creation IPC, version-detection queries, proxy get/save, sync and migration status helpers, Claude profile resolution, debug decryption, direct SSH list wrappers, the unused Skills and Subagents config, export, local scan/import, update-check, rescan and repository-refresh commands, the Skills update diff/apply pair, and the Webhook stop wrapper. Removing a command is an intentional interface withdrawal: an old caller now receives the standard unknown-command error.
- Preserve reachable workflows: Subagents update preview/apply and both folder imports, internal pre-restore backup creation, SSH snapshot plus common-port upsert/delete, MCP template catalog/list/get with managed servers and messages, and native shutdown.
- Route Webhook listener stopping through native shutdown: `shutdown_runtime_services` idempotently calls `WebhookServerManager::global().stop()` instead of exposing a stop IPC.
- Make `BackupManager` an all-backups manager that lists every backup and keeps restore, delete and retention cleanup. Remove manual creation, the ignored active-tool prop and the missing-tool warning; keep the pre-restore safety backup internal to the backend.
- Remove the ignored model-pool card/list delete callback chain while keeping deletion in the model-pool editor, and remove the model switcher's unused visibility field without breaking registry loading.
- Remove audited orphan UI modules, types, CSS rules, npm and Cargo dependencies, assets and root prompt/script files, and the audited i18n keys. Preserve dynamic families (for example runtime-composed workspace skill rule keys) and shared live keys.
- No persistent format, migration or encryption behavior changed; stored user data, legacy readers and existing schema versions remain intact.

## Alternatives considered

- Keep the dormant MCP runtime, matcher and unused command interfaces for possible future use: declined because no production consumer existed, and dormant public interfaces still owe compatibility, review and maintenance without serving a workflow.
- Delete by name, including callbacks that look unused: declined because the Subagents preview/apply workflow, folder imports, pre-restore backup, snapshot/common-port mutations and model-pool editor deletion are reachable and would break.
- Remove every key under a shared or dynamic i18n prefix: declined because runtime-composed keys and shared keys are live and cannot be proven unused by literal search alone.
- Upgrade dependencies broadly while cleaning up: declined because the cleanup needed only removal of unreachable direct dependencies; unrelated version movement would add risk outside the audited scope.
- Delete the Subagents update workflow or the pre-restore backup as part of the cleanup: declined because both are live behaviors, and the backup is the safety net for restoration.

## Consequences

- Every preserved workflow remains usable, while the removed surfaces no longer add build, test or review cost.
- Old callers of a retired IPC name receive the standard unknown-command error instead of a compatibility shim; this is an intentional interface change, and reverting the cleanup commits restores the code and interfaces.
- Verification evidence: the scoped Rust suites passed (app_runtime 44, mcp_templates 1, mcp_servers 17, skills 47, subagents 17, ssh_tunnels 68, protocol_router 16, backup 5, webhook_catcher 3, app_store 135), the frontend gate passed 10 files and 362 tests, `npm run build` exited 0, and the i18n bundle moved from 3,534 to 3,368 leaf keys per language with identical English/Chinese sets and 1,038 key-list entries. The dependency cleanup removed four npm packages and two direct Rust crates without upgrading retained packages.
- Known caveat: `npm run lint` remains exit 1 with one pre-existing `react-hooks/globals` error in the untouched `src/components/AiGateway/UsageLogsPanel.test.tsx` plus pre-existing warnings; the cleanup did not introduce that error.
- Partial supersession: [AI Workspace Removed End to End](2026-09-29-ai-workspace-removal.md) remains authoritative for the workspace removal, and only its former retained-runtime statements are repaired and cross-linked here; both records stay active. No persistent format, migration or encryption change was made.
- Reintroduction condition: a future feature that needs the dormant MCP client or matcher must add it back explicitly with a production consumer and behavior tests, not by restoring the deleted module.
