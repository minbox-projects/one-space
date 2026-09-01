---
plan_id: "20260901-jtt-data-parser"
status: frozen
created_at: "2026-09-01T00:00:00Z"
supersedes: null
requirement_count: 13
acceptance_criteria_count: 36
digest: "sha256:2da2f460a908050e873bd9c81d0bc08b28ad4f020833c19bef583811d0bc98a0"
---

# Implementation Plan

## Requirement coverage

| Requirement | Acceptance criteria | Implementation step | Validation |
| --- | --- | --- | --- |
| REQ-001 | AC-001, AC-002 | Steps 2, 3 | Parser component and More Tools tests |
| REQ-002 | AC-003 to AC-005 | Step 3 | Navigation, App, Launcher, visibility, and hub tests |
| REQ-003 | AC-006, AC-007 | Steps 1, 2, 5 | Pure parser, component lifecycle, and changed-path audit |
| REQ-004 | AC-008 to AC-011 | Steps 1, 2 | JT808 fixtures and selector tests |
| REQ-005 | AC-012 to AC-016 | Steps 1, 2 | JT809 fixtures, parameter boundaries, and controls |
| REQ-006 | AC-017 to AC-019 | Steps 1, 2 | JT1078 operation/direction fixtures and UI tests |
| REQ-007 | AC-020 to AC-022 | Steps 1, 2 | Hex unit and component tests |
| REQ-008 | AC-023 to AC-025 | Steps 1, 2 | Lexical and subpackage tests |
| REQ-009 | AC-026, AC-027 | Steps 1, 2 | Boundary and counterexample tests |
| REQ-010 | AC-028 to AC-030 | Step 2 | Tree rendering and clipboard serialization tests |
| REQ-011 | AC-031, AC-032 | Step 2 | Action-state and Toast/clipboard tests |
| REQ-012 | AC-033, AC-034 | Steps 1, 4 | Network-denied fixture and behavior suite |
| REQ-013 | AC-035, AC-036 | Steps 3, 5 | Build, desktop smoke, visibility, and changed-path audit |

## Implementation sequence

### Step 1: Independently implement the pure local parser domain

- Responsible role: `frontend`.
- Read scope: `src/components/JsonParserTool.tsx`, `src/components/JsonParserTool.test.tsx`, `src/test/mocks/render.tsx`, `src/lib/navigation.ts`, `src/lib/moreToolPresentation.ts`; lawfully accessible public texts for JT/T 808-2011/2013/2019, JT/T 809-2011/2019, JT/T 1078-2016, and the approved public extension specifications. Internal research may inspect only the frozen upstream commit named in `spec.md`; no upstream code, test, text, fixture, sample, brand, link, attribution, or license content may enter a repository write scope.
- Write scope: new bounded directory `src/lib/jttDataParser/` and its co-located independently authored Vitest tests and fixtures.
- Changes: create pure synchronous functions for ASCII whitespace treatment, LF and CRLF lexical behavior, Hex conversion, UTF-8 validation including isolated surrogate rejection, structured Chinese result records, plain-text result serialization, JT808 five-mode frame/package behavior, `0x0801` handling, bounded body-state records, JT809 frame behavior and `uint32` parameter validation, JT809-2019 `0x0200` bridge, and JT1078 four-operation/direction behavior. Fixtures must cover each support-matrix row, supported result, unsupported-body result, invalid boundary, and legal counterexample.
- Validation: run the exact targeted Vitest command created for `src/lib/jttDataParser/` with `fetch`, `XMLHttpRequest`, and parser-related Tauri invocation denied in the test environment. It proves AC-006, AC-008 to AC-027, AC-030, AC-033, and AC-034, and fails RED before the new module and fixtures exist.
- Dependencies: none.

### Step 2: Build the OneSpace four-tab parser interface

- Responsible role: `frontend`.
- Read scope: `src/components/JsonParserTool.tsx`, `src/components/JsonParserTool.test.tsx`, `src/components/MoreToolsHub.tsx`, `src/lib/moreToolPresentation.ts`, `src/test/mocks/render.tsx`, `src/lib/jttDataParser/`.
- Write scope: `src/components/JttDataParserTool.tsx`, `src/components/JttDataParserTool.test.tsx`.
- Changes: implement OneSpace-styled tabs with local mounted-component state. Add working JT808 and JT809 top selects, JT809 decimal `M1`/`IA1`/`IC1` controls, JT1078 operation and direction controls, Hex direction, editable text areas, inline errors, ordered Chinese trees, and action buttons. On selector changes, retain raw input and clear that tab's result/error. Implement the specified clear lifecycle, Hex-only independently authored example, and copy result serialization with existing Toast feedback. Do not add third-party references, stored data, JSON/summary views, background execution, or cross-tab mutation.
- Validation: targeted component tests prove AC-002, AC-007, AC-010, AC-012, AC-014 to AC-022, AC-025 to AC-032, and AC-033. Tests explicitly unmount/remount, simulate clipboard success/no-result/rejection, and assert one tab's action cannot mutate another tab.
- Dependencies: Step 1.

### Step 3: Register the single tool and optional navigation subtab

- Responsible role: `frontend`.
- Read scope: `src/App.tsx`, `src/components/MoreToolsHub.tsx`, `src/components/MoreToolsHub.test.tsx`, `src/components/Launcher.tsx`, `src/lib/navigation.ts`, `src/lib/navigation.test.ts`, `src/lib/moreToolPresentation.ts`, `src/lib/launcherToolVisibility.ts`, `src/components/JttDataParserTool.tsx`, and the exact existing Launcher test file if one is present at implementation time.
- Write scope: `src/App.tsx`, `src/components/MoreToolsHub.tsx`, `src/components/MoreToolsHub.test.tsx`, `src/components/Launcher.tsx`, `src/lib/navigation.ts`, `src/lib/navigation.test.ts`, `src/lib/moreToolPresentation.ts`, `src/lib/launcherToolVisibility.ts`, plus the exact existing Launcher test file read in this step or a new `src/components/Launcher.test.tsx` if no suitable existing file exists.
- Changes: add one `jtt-data-parser` More Tools section and presentation entry, a default-true visibility key, catalog card and detail dispatch, a Launcher total entry with searchable aliases, and optional `jttParserTab` in the resolved navigation target. Update App shell state and MoreToolsHub props so the optional subtab reaches the one tool component. Alias targets `808`, `809`, `1078`, and `hex` must carry the matching subtab; the total target carries none and defaults to JT808. The visibility switch hides only Launcher entries, not the catalog card, so it remains recoverable. Preserve every existing navigation target and stored visibility behavior.
- Validation: targeted navigation, App/MoreToolsHub, Launcher, and visibility tests prove AC-001, AC-003 to AC-005, AC-007, AC-010, AC-017, AC-022, AC-035, and AC-036. Include legacy localStorage fixtures with a missing JT/T key and an obsolete JT/T key. These tests fail RED before registrations and optional subtab transport exist.
- Dependencies: Step 2.

### Step 4: Run the focused offline front-end regression suite

- Responsible role: `test`.
- Read scope: `src/lib/jttDataParser/`, `src/components/JttDataParserTool.tsx`, `src/components/JttDataParserTool.test.tsx`, `src/components/MoreToolsHub.test.tsx`, `src/lib/navigation.test.ts`, and the exact Launcher test file changed in Step 3.
- Write scope: none.
- Changes: none.
- Validation: run one repository-native targeted Vitest command that includes the parser directory, `JttDataParserTool.test.tsx`, `MoreToolsHub.test.tsx`, `navigation.test.ts`, and the Launcher test changed in Step 3 while network and parser-related Tauri calls are denied. It detects parser regressions, unsupported-body misclassification, lifecycle leaks, line/package rule errors, navigation/subtab failures, visibility recovery failures, and clipboard feedback failures. Passing evidence proves AC-001 through AC-034 that are testable in the front-end environment.
- Dependencies: Steps 1 through 3.

### Step 5: Verify the front-end boundary and desktop compatibility evidence

- Responsible role: `test`.
- Read scope: `package.json`, `src/lib/jttDataParser/`, `src/components/JttDataParserTool.tsx`, `src/components/MoreToolsHub.tsx`, `src/components/Launcher.tsx`, `src/App.tsx`, `src/lib/navigation.ts`, `src/lib/moreToolPresentation.ts`, `src/lib/launcherToolVisibility.ts`, and the immutable changed-path evidence supplied by the native `git-operator` after implementation.
- Write scope: none.
- Changes: none.
- Validation: run `npm run build` to detect TypeScript, import, and bundle failures. Run the repository's existing Tauri 2 desktop smoke/package procedure for its supported macOS, Windows, and Linux contract; report any unavailable host-specific check rather than treating the front-end build as equivalent. Review immutable changed-path evidence to confirm no `src-tauri`, database, configuration, external endpoint, packet persistence, task-system, or third-party artifact changed. This evidence proves AC-006, AC-033 to AC-036.
- Dependencies: Step 4.

## Integration and compatibility

The parser is a front-end-only module and has no typed IPC facade, Tauri command, database contract, or external dependency. The stable tool ID is `jtt-data-parser`; `ResolvedNavigationTarget.jttParserTab` is optional so current consumers and total tool navigation remain valid. App owns the active More Tools section and optional JT/T subtab state, then passes it to `MoreToolsHub` and the parser component.

The existing Launcher visibility record adds one default-true JT/T key. Existing objects missing the field read as visible. The More Tools card intentionally ignores that field so its detail switch remains a recovery path. Parser session data belongs only to the mounted parser component and must never be elevated into localStorage or app-wide persistent state.

## Rollback

Remove `src/lib/jttDataParser/`, `JttDataParserTool`, their tests, the optional `jttParserTab` field/state/prop transport, and the single JT/T catalog, Launcher, presentation, and visibility registrations. Do not alter Tauri, Rust, database, service, configuration, packet, result, or parameter data because none exists outside the mounted component. Re-run existing More Tools, Launcher, navigation, visibility, test, build, and available desktop smoke checks. A visibility object containing the removed JT/T field remains safe because the existing reader uses only recognized defaults.

## Risks

| Risk | Mitigation | Evidence |
| --- | --- | --- |
| Public message-body coverage is broader than what the frozen research can independently enumerate. | Implement only the named frame, package, operation, and bridge scope; render other valid bodies as explicit unsupported states. | Per-row fixtures prove support and unsupported-body results. |
| JT809 encrypted body decryption has no auditable public parameter-to-cipher contract. | Validate decimal `uint32` inputs and render only frame-level encrypted state. Never label the body as decrypted. | JT809 encrypted fixture and parameter-boundary tests. |
| Optional subtab routing breaks current More Tools targets. | Use one tool ID plus an optional target field; test new aliases and pre-existing resolver behavior. | Navigation, App, More Tools, and Launcher tests. |
| Existing launcher visibility values become unreadable. | Add one default-true key through the current defensive reader and test missing/obsolete fields. | Visibility regression tests. |
| Synchronous parsing has poor behavior on large pasted data. | Do not add a new execution model or artificial cap; verify representative frozen samples complete without crash and that post-completion actions work. | AC-033 behavior test and desktop smoke. |
| Independent-implementation boundary is violated by upstream artifacts. | Limit upstream use to internal research, author fixtures/text independently, and audit changed paths and provenance before completion. | Git Operator changed-path evidence and Test audit. |
