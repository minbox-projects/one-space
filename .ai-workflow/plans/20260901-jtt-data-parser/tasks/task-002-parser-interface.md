---
id: "task-002-parser-interface"
requirements: ["REQ-001", "REQ-003", "REQ-004", "REQ-005", "REQ-006", "REQ-007", "REQ-008", "REQ-009", "REQ-010", "REQ-011", "REQ-012"]
acceptance_criteria: ["AC-002", "AC-007", "AC-008", "AC-010", "AC-012", "AC-014", "AC-016", "AC-017", "AC-018", "AC-019", "AC-020", "AC-021", "AC-022", "AC-025", "AC-026", "AC-027", "AC-028", "AC-029", "AC-030", "AC-031", "AC-032", "AC-033"]
depends_on: ["task-001-parser-domain"]
surface: frontend
feature: "more-tools"
locator_read_order: ["src/components/JsonParserTool.tsx", "src/components/JsonParserTool.test.tsx", "src/test/mocks/render.tsx", "src/lib/navigation.ts", "src/lib/moreToolPresentation.ts", "src/lib/launcherToolVisibility.ts", "src/lib/jttDataParser/", "src/components/JttDataParserTool.tsx", "src/components/JttDataParserTool.test.tsx", "src/components/MoreToolsHub.tsx", "src/components/MoreToolsHub.test.tsx", "src/components/Launcher.tsx", "src/components/Launcher.test.tsx", "src/lib/navigation.test.ts", "src/lib/launcherToolVisibility.test.ts", "src/App.tsx"]
read_scope: ["MEMORY.md", ".ai-workflow/index/navigation.json", ".ai-workflow/index/navigation.md", "src/components/JsonParserTool.tsx", "src/components/JsonParserTool.test.tsx", "src/test/mocks/render.tsx", "src/lib/navigation.ts", "src/lib/moreToolPresentation.ts", "src/lib/launcherToolVisibility.ts", "src/lib/jttDataParser/", "src/components/JttDataParserTool.tsx", "src/components/JttDataParserTool.test.tsx", "src/components/MoreToolsHub.tsx", "src/components/MoreToolsHub.test.tsx", "src/components/Launcher.tsx", "src/components/Launcher.test.tsx", "src/lib/navigation.test.ts", "src/lib/launcherToolVisibility.test.ts", "src/App.tsx"]
write_scope: ["src/components/JttDataParserTool.tsx", "src/components/JttDataParserTool.test.tsx"]
test_commands: ["npm run test -- src/components/JttDataParserTool.test.tsx"]
---

# Task

## Objective

Build the OneSpace-styled four-tab `JT/T 数据解析` component that consumes the parser domain and exposes the frozen JT808, JT809, JT1078, and Hex workflows with mounted-session state, stable Chinese result trees, and recoverable clipboard feedback.

## Implementation notes

- Keep one local state model per tab. Switching tabs preserves raw input, controls, results, errors, and JT809 parameters; component unmount followed by remount returns every tab to its initial state.
- Provide working JT808 mode and JT809 version/encryption selectors at the top of their tabs. In JT809 encrypted mode show decimal `M1`, `IA1`, and `IC1` controls only. Provide JT1078 operation/direction and Hex direction controls.
- On any option change, retain only that tab's raw input and clear only its result/error. Analyze updates only the current tab. Clear resets only the current tab, including JT809 parameters. Only Hex has an independently authored example.
- Render success, unsupported, and error records as a stable Chinese hierarchy. Copy the current tab's complete semantic results as the parser domain's indented plain text, using existing OneSpace Toast feedback.
- Preserve raw input and prior results for validation errors and copy failures. Do not add JSON/summary modes, persistent storage, external links, remote calls, Worker/task state, or cross-tab mutation.

## Negative cases

- Ensure private JT808 options are absent, encrypted JT809 never claims body decryption, and unsupported bodies remain visibly diagnosable.
- Test empty/no-result copy, clipboard rejection, malformed input, invalid JT809 values, invalid UTF-8, internal JT809/JT1078 newlines, and mismatched JT1078 direction without clearing user input.
- Verify selector changes clear stale result/error but do not replace the user's original packet with a sample.

## Test evidence

- `npm run test -- src/components/JttDataParserTool.test.tsx` proves tab-local mounted state, remount reset, selectors, clear/example behavior, input/error retention, tree output, and clipboard success/no-result/failure feedback.
- The component suite consumes task 001's independent fixtures to demonstrate rendered behavior for the shared parser semantics in AC-008, AC-018, AC-026, AC-030, and AC-033.

## Completion definition

- All assigned REQ and AC identifiers are represented in the component behavior and tests.
- Changes remain within the two approved component paths.
- The assigned test command passes after task 001's parser-domain command has passed.
