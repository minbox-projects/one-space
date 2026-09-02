---
id: "task-003-register-parser-tool"
requirements: ["REQ-001", "REQ-002", "REQ-013"]
acceptance_criteria: ["AC-001", "AC-003", "AC-004", "AC-005", "AC-035", "AC-036"]
depends_on: ["task-002-parser-interface"]
surface: frontend
feature: "more-tools"
locator_read_order: ["src/App.tsx", "src/components/MoreToolsHub.tsx", "src/components/Launcher.tsx", "src/lib/navigation.ts", "src/lib/launcherToolVisibility.ts", "src/lib/moreToolPresentation.ts", "src/App.moreToolsNavigation.test.tsx", "src/components/MoreToolsHub.test.tsx", "src/components/Launcher.test.tsx", "src/lib/navigation.test.ts", "src/lib/launcherToolVisibility.test.ts"]
read_scope: ["MEMORY.md", ".ai-workflow/index/navigation.json", ".ai-workflow/index/navigation.md", "src/App.tsx", "src/components/MoreToolsHub.tsx", "src/components/Launcher.tsx", "src/lib/navigation.ts", "src/lib/launcherToolVisibility.ts", "src/lib/moreToolPresentation.ts", "src/App.moreToolsNavigation.test.tsx", "src/components/MoreToolsHub.test.tsx", "src/components/Launcher.test.tsx", "src/lib/navigation.test.ts", "src/lib/launcherToolVisibility.test.ts"]
new_module_directories: []
write_scope: ["src/App.tsx", "src/components/MoreToolsHub.tsx", "src/components/MoreToolsHub.test.tsx", "src/components/Launcher.tsx", "src/components/Launcher.test.tsx", "src/lib/navigation.ts", "src/lib/navigation.test.ts", "src/lib/moreToolPresentation.ts", "src/lib/launcherToolVisibility.ts", "src/lib/launcherToolVisibility.test.ts"]
test_commands: ["npm run test -- src/components/JttDataParserTool.test.tsx src/components/MoreToolsHub.test.tsx src/lib/navigation.test.ts src/components/Launcher.test.tsx src/lib/launcherToolVisibility.test.ts", "npm run build"]
---

# Task

## Objective

Register the completed `JT/T 数据解析` component as one recoverable More Tools and Launcher tool, with optional navigation subtab routing for protocol aliases and no backend, persistence, or desktop-contract expansion.

## Implementation notes

- Add exactly one `jtt-data-parser` More Tools section, presentation/icon entry, catalog card, detail dispatch, Launcher entry, and default-true Launcher visibility field.
- Extend the resolved navigation target with optional `jttParserTab`. The total JT/T entry has no subtab and defaults to JT808; aliases `808`, `809`, `1078`, and `hex` use the same tool ID with the matching subtab.
- Keep App as the owner of active More Tools section and optional JT/T subtab state. Thread the optional value through MoreToolsHub into the parser component without changing existing targets.
- The detail switch hides JT/T Launcher results only. The More Tools catalog card must stay visible so users can reopen the tool and restore Launcher visibility.
- Preserve existing visibility records: a missing JT/T key defaults to visible, while obsolete keys are ignored. Do not add Tauri commands, Rust modules, database changes, configuration, packet storage, network endpoints, or third-party material.

## Negative cases

- Existing More Tools and independent sidebar targets must preserve their current resolver output.
- Alias navigation must not create four pseudo-tool IDs, duplicate cards, or route `Hex` to the wrong tab.
- Hiding JT/T from Launcher must not hide its More Tools recovery path. Visibility records with missing or obsolete JT/T fields must remain readable.
- Feature rollback must require only removal of the named front-end registrations and modules; no protocol data or backend migration may be introduced.

## Test evidence

- `npm run test -- src/components/JttDataParserTool.test.tsx src/components/MoreToolsHub.test.tsx src/lib/navigation.test.ts src/components/Launcher.test.tsx src/lib/launcherToolVisibility.test.ts` proves the component remains callable, the catalog has one card, total/alias routing transports `jttParserTab`, and visibility is recoverable and backward-compatible.
- `npm run build` proves TypeScript and Vite integration contains no broken imports or unsupported front-end contract.
- Run the existing Tauri 2 desktop smoke/package procedure on available macOS, Windows, and Linux hosts; record unavailable host-specific evidence rather than treating the front-end build as equivalent.

## Completion definition

- All assigned REQ and AC identifiers are covered by integration tests and build evidence.
- Changes remain within the ten approved registration and test paths.
- All `test_commands` pass, and available desktop smoke evidence confirms no new backend or migration contract.
