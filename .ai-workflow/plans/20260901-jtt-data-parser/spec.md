---
plan_id: "20260901-jtt-data-parser"
status: frozen
created_at: "2026-09-01T00:00:00Z"
supersedes: null
requirement_count: 13
acceptance_criteria_count: 36
digest: "sha256:cf6b9ce6804579bdff2cd205dcc64b864fdc5d28e46f9a363e287c0ac7ec7a9d"
---

# Specification

## Goal

Add one local-only More Tools entry named `JT/T 数据解析` for OneSpace desktop users who develop, integrate, or operate vehicle telematics protocols. The tool provides independent JT808, JT809, JT1078, and Hex tabs, follows existing OneSpace visual patterns, supports Launcher search and protocol shortcuts, and processes packet data synchronously in front-end memory without persisting packets, results, or JT809 parameters.

Success is measured by offline fixtures and UI behavior tests for every frozen support-matrix row, navigation target, input boundary, state transition, clipboard result, and rollback condition in this document.

## Non-goals

- Do not add a Web or mobile application, remote parsing service, provider API, service configuration, Tauri command, Rust module, database, migration, export, history, autosave, task queue, Worker, cancellation, or OneSpace-specific input limit.
- Do not add vendor plug-ins, vendor protocol configuration, Ruiding, GPS51, or any other private mode without a public and approved specification.
- Do not copy, distribute, or present JTTools code, tests, wording, samples, branding, links, attribution, or license content. The internal research reference `WebApi` commit `e2e0f626e74c7b5fb18f1fdcd1f7351c78783336` is not a product artifact. Public standards override it for field meaning, validation, and errors.
- Do not provide a summary view, JSON result view, result export, or persistent protocol data. The existing Launcher visibility preference remains the only persisted setting involved.
- Do not promise support for protocol message bodies outside the frozen support matrix.

## Scenarios

### Primary scenario

A protocol developer opens `JT/T 数据解析` from More Tools, or searches the Launcher for the tool name or a protocol keyword. They select the relevant tab and mode, paste packet data, run local analysis, inspect a complete Chinese tree result, and copy the current tab's result as indented plain text.

### Batch scenario

A developer pastes multiple JT808 packets on separate lines. Each nonblank line receives an ordered success, frame-level unsupported-body state, or specific error without deleting the original batch or successful neighboring results.

### Encrypted JT809 scenario

A developer selects JT809 encrypted mode and supplies `M1`, `IA1`, and `IC1` as in-memory decimal `uint32` values. The tool shows publicly determinable frame fields and explicitly marks the encrypted body as not decryptable under the frozen scope; it never claims that it decrypted an unsupported body.

### Recovery scenario

A user enters malformed data, changes an option, or encounters a clipboard denial. The tool keeps the relevant raw input, reports the actionable problem inline or through OneSpace feedback, and allows correction. Switching tabs preserves tab-local state while the tool remains mounted; leaving More Tools or another unmounting action clears the whole tool session.

## Frozen support matrix

| Tab and mode | Supported behavior | Explicit unsupported behavior |
| --- | --- | --- |
| JT808 automatic | Frame-level JT/T 808-2011, 2013, and 2019 recognition; newline records; public framing, escaping, checksum, package metadata, and `0x0801` first-fragment body handling. | An unlisted body shows frame fields, message ID, raw body Hex, and an unsupported-body state. |
| JT808 JT1078 extension | The selected mode applies the independently implemented public extension framing/body rules that are frozen by its fixtures. | Unlisted bodies follow the unsupported-body state. |
| JT808 Jiangsu active safety | The selected mode applies the independently implemented public active-safety extension rules frozen by its fixtures. | Unlisted bodies follow the unsupported-body state. |
| JT808 Guangdong active safety | The selected mode applies the independently implemented public active-safety extension rules frozen by its fixtures. | Unlisted bodies follow the unsupported-body state. |
| JT808 force 2013 | The selected mode applies 2013 version interpretation to the supported frame and body scope. | Unlisted bodies follow the unsupported-body state. |
| JT809 2011/2019 unencrypted | One trimmed packet; public frame-level structure; JT809-2019 `0x0200` bridge to the frozen JT808 `0x0200` formatter. | Other bodies expose known frame structure, message ID, raw body Hex, and unsupported-body state. |
| JT809 encrypted | Both versions retain encrypted selection. `M1`, `IA1`, and `IC1` are session-only decimal `uint32` values. Publicly determinable frame fields are shown. | The encrypted body is marked as lacking an auditable public decryption specification; no full-body decryption claim is made. |
| JT1078 | One trimmed JT/T 1078-2016 packet. The visible JT808 linkage operations are `0x9101`, `0x9102`, `0x9205`, and `0x9206`, with up/down direction selection affecting public-rule interpretation. | Other bodies expose known frame fields, message ID, raw body Hex, and unsupported-body state. |
| Hex | Independently implemented `Hex -> UTF-8` and `UTF-8 -> Hex` conversion. | Invalid Hex, odd Hex digit count, invalid UTF-8, and isolated UTF-16 surrogates fail inline. |

## Requirements

### REQ-001: Register one four-tab JT/T tool

Expose exactly one More Tools entry named `JT/T 数据解析` and provide JT808, JT809, JT1078, and Hex workspaces.

### REQ-002: Integrate Launcher navigation and recoverable visibility

Use one stable More Tools tool ID with an optional `jttParserTab` navigation target. The tool is visible in More Tools by default and its Launcher entry defaults to visible.

### REQ-003: Keep packet work local and ephemeral

Process packets and JT809 parameters only in front-end memory. Preserve tab-local state while the tool is mounted, and reset it on tool component unmount.

### REQ-004: Parse the approved JT808 modes and frozen frame scope

Provide a working top selector for automatic JT/T 808-2011/2013/2019 recognition, JT1078 extension, Jiangsu active safety, Guangdong active safety, and force-2013. The selector must change the next analysis and exclude Ruiding and GPS51.

### REQ-005: Parse JT809 versions and constrained encrypted state

Provide working JT809 2011/2019 and unencrypted/encrypted selectors. Encrypted mode uses only the in-memory decimal `uint32` fields `M1`, `IA1`, and `IC1`, and exposes frame-level information without claiming unsupported decryption.

### REQ-006: Parse JT1078 and its visible JT808 linkage operations

Provide JT/T 1078-2016 frame parsing with `0x9101`, `0x9102`, `0x9205`, and `0x9206` operation choices and upstream/downstream direction selection.

### REQ-007: Convert Hex and UTF-8 in both directions

Provide explicit `Hex -> UTF-8` and `UTF-8 -> Hex` modes, line-preserving conversion, an independently authored Hex example, clear behavior, and copy behavior.

### REQ-008: Apply deterministic line and package rules

Apply the approved lexical rules to JT808 batches, JT809/JT1078 single packets, Hex lines, and JT808 subpackages.

### REQ-009: Report actionable validation and scope errors

Keep raw input intact and explain invalid input, packet framing, mode/direction mismatch, unavailable decryption, and unsupported bodies.

### REQ-010: Render and copy a stable Chinese result tree

Render all current-tab success, unsupported, and error records in a stable Chinese tree with raw values and required interpretation. Copy the same semantic content as indented plain text.

### REQ-011: Define tab-local action and clipboard state changes

Keep analyze, clear, example, option changes, and copy actions tab-local and give recoverable success or error feedback.

### REQ-012: Verify synchronously with independent offline fixtures

Keep the execution model synchronous and verify the frozen support matrix entirely with independently authored, version-controlled fixtures that do not use an external service.

### REQ-013: Preserve desktop compatibility and isolated rollback

Keep the current Tauri desktop support contract intact and allow the feature to be removed using only front-end registrations and modules.

## Acceptance criteria

### AC-001: One catalog card

- Given the More Tools catalog is open
- When the tool is visible
- Then exactly one `JT/T 数据解析` card follows the existing card and icon presentation pattern.

### AC-002: Mounted tab state is independent

- Given the mounted tool contains work in more than one tab
- When the user switches away from and back to a tab
- Then that tab restores its own input, selected controls, current result, error, and JT809 parameter state.

### AC-003: General Launcher entry opens JT808

- Given Launcher search matches `JT/T 数据解析`
- When the user opens the result
- Then the single `jtt-data-parser` tool opens with JT808 selected.

### AC-004: Protocol Launcher aliases select a subtab

- Given Launcher search matches `808`, `809`, `1078`, or `Hex`
- When the user opens the matching result
- Then navigation has the same `jtt-data-parser` More Tools section and the matching optional `jttParserTab` value.

### AC-005: Visibility is recoverable from More Tools

- Given the user disables `在启动台展示` in the JT/T tool detail
- When the user searches Launcher
- Then JT/T entries are absent from Launcher while its More Tools card and detail switch remain visible to re-enable them.

### AC-006: Parsing has no remote or persisted protocol path

- Given the user analyzes or converts any supported input
- When the action runs
- Then it makes no network request, invokes no Tauri parser command, and writes no packet, result, or JT809 parameter to localStorage.

### AC-007: Unmount resets the complete session

- Given the user has entered data, results, controls, and JT809 parameters
- When navigation unmounts the parser and the user opens it again
- Then all four tabs start from their initial state.

### AC-008: JT808 selected modes parse frozen frame scope

- Given a valid JT808 fixture for a selected supported mode
- When the user analyzes it
- Then the tree displays the public frame, escaping, checksum, package, and frozen `0x0801` information for that mode.

### AC-009: Automatic and force-2013 behavior differs when applicable

- Given a version-sensitive JT808 fixture
- When automatic recognition and force-2013 run separately
- Then each result follows the corresponding public-version interpretation.

### AC-010: Private JT808 modes are absent

- Given the JT808 mode selector is open
- When the user reads its choices
- Then Ruiding and GPS51 are not selectable.

### AC-011: Unknown JT808 body remains diagnosable

- Given a structurally valid JT808 frame with a body outside the frozen scope
- When analyzed
- Then the result shows frame fields, message ID, raw body Hex, and a clear unsupported-body state instead of invented body fields.

### AC-012: JT809 top selectors affect next analysis

- Given the JT809 tab is active
- When the user inspects or changes 2011/2019 and unencrypted/encrypted selectors
- Then all four choices exist and a changed value clears that tab's result and error without changing its raw input.

### AC-013: JT809 unencrypted frozen scope is rendered

- Given an unencrypted JT809 fixture for the selected version
- When analyzed
- Then the result shows its frozen frame-level tree and a JT809-2019 `0x0200` fixture shows the frozen bridge result.

### AC-014: Encrypted JT809 inputs are constrained and ephemeral

- Given encrypted JT809 is selected
- When the user inspects parameter controls
- Then `M1`, `IA1`, and `IC1` are visible, default to decimal `0`, accept only integers from `0` through `4294967295`, and exist only while the tool is mounted.

### AC-015: Encrypted JT809 avoids false decryption claims

- Given encrypted JT809 has three valid parameters and a structurally valid packet
- When analyzed
- Then known frame fields render and the encrypted body explicitly reports that full decryption is outside the auditable frozen scope.

### AC-016: Invalid JT809 parameters are retained and explained

- Given encrypted JT809 has a missing, blank, signed, fractional, nonnumeric, or out-of-range parameter
- When analysis is requested
- Then a concrete inline parameter error appears and the input, selectors, values, and prior result remain unchanged.

### AC-017: JT1078 linkage controls are visible

- Given the JT1078 tab is active
- When the user inspects its controls
- Then `0x9101`, `0x9102`, `0x9205`, `0x9206`, upstream, and downstream choices are available.

### AC-018: JT1078 selected direction and operation change parsing

- Given a valid fixture for one frozen linkage operation and a matching direction
- When analyzed
- Then the tree renders the corresponding public frame and frozen operation fields; a direction mismatch produces a specific error.

### AC-019: Unknown JT1078 body remains diagnosable

- Given a structurally valid JT1078 frame outside the frozen body scope
- When analyzed
- Then the result shows known frame fields, message ID, raw body Hex, and unsupported-body state.

### AC-020: Hex-to-UTF-8 preserves LF line positions

- Given `Hex -> UTF-8` and LF-separated Hex lines
- When conversion runs
- Then ASCII whitespace inside nonblank lines is ignored, valid lines decode in order, and blank lines remain blank output lines.

### AC-021: UTF-8-to-Hex has stable output formatting

- Given `UTF-8 -> Hex` and LF-separated text
- When conversion runs
- Then each line becomes uppercase byte pairs separated by one ASCII space, in the same order with blank lines preserved.

### AC-022: Hex actions exist

- Given the Hex tab is active
- When the user inspects it
- Then explicit direction selection, conversion, clear, an independently authored example, and copy are available.

### AC-023: JT808 lexical line behavior is stable

- Given JT808 input containing LF or CRLF, blank lines, whitespace-only lines, or a trailing newline
- When analyzed
- Then only nonblank lines produce ordered records and each record uses its original one-based line number.

### AC-024: JT808 package assembly has explicit boundaries

- Given JT808 fragments share session, version, terminal ID, message ID, serial number, and declared total
- When all distinct continuous indexes from `1` through `N` are present
- Then they merge by index; duplicate indexes, declared-total conflicts, or missing indexes do not merge and show their concrete package state.

### AC-025: JT809 and JT1078 reject multi-line packets

- Given JT809 or JT1078 input contains one or more internal newline characters after trimming outer ASCII whitespace
- When analysis is requested
- Then it fails with a single-packet error and does not split the input.

### AC-026: Invalid boundaries retain raw input

- Given empty input, non-Hex data, odd Hex length, truncation, checksum failure, mode/direction mismatch, invalid UTF-8, isolated surrogate, JT809 parameter error, or private-mode attempt
- When the applicable operation runs
- Then a specific error is visible without changing the raw input or current selection.

### AC-027: Valid counterexamples pass their guard

- Given an even-length valid Hex sequence, a complete matching frame, valid UTF-8 text, valid JT809 decimal parameters, and a matching direction
- When operated
- Then the corresponding invalid-boundary error is not reported.

### AC-028: Result tree is stable and complete for the frozen scope

- Given a success, unsupported-body state, or line error
- When the user reads the result
- Then a stable Chinese tree includes required labels, raw values, interpretations, and one-based line context.

### AC-029: Batch results remain separate and ordered

- Given JT808 produces multiple records
- When results render
- Then each record remains independently ordered by input line and no JSON or summary result mode is offered.

### AC-030: Copy serializes the displayed semantic result

- Given the current tab has one or more result records
- When the user copies them
- Then the clipboard receives an indented plain-text tree containing Chinese labels, raw values, interpretations, line numbers, and error records.

### AC-031: Actions only mutate the current tab

- Given any tab has state
- When Analyze, Clear, Example, or a selector change is invoked
- Then only that tab changes: Analyze updates its result; Clear resets its input, controls, result, error, and JT809 parameters; only Hex has an example; selector changes retain raw input and clear that tab's result and error.

### AC-032: Clipboard feedback is recoverable

- Given the current tab has a result, no result, or a denied clipboard write
- When Copy is used
- Then OneSpace feedback reports success or the actual failure and does not clear input or results.

### AC-033: Fixed samples complete safely without an execution system

- Given committed single-packet, batch, package, encrypted-frame, and Hex fixtures
- When each executes in a network-denied test environment
- Then it completes without an application crash and, after completion, the user can edit, switch tabs, and analyze again.

### AC-034: Offline fixtures cover every frozen scope row

- Given the parser test suite runs without network access
- When it exercises the support matrix
- Then independently authored fixtures cover supported success, unsupported body, boundary error, and legal counterexample for every frozen mode.

### AC-035: Existing desktop contract remains front-end only

- Given the current macOS, Windows, and Linux Tauri 2 support contract
- When the parser is built and exercised through the existing desktop smoke path
- Then it requires no new backend, service, database, migration, or command.

### AC-036: Rollback has no protocol data recovery

- Given the JT/T module and all of its front-end registrations are removed
- When existing More Tools, Launcher, navigation, visibility, and build checks run
- Then no protocol data needs recovery and visibility objects containing either missing or obsolete JT/T fields are ignored safely.

## Error boundaries and counterexamples

- Empty or ASCII-whitespace-only input fails; a complete matching packet does not.
- A one-digit or odd-digit Hex line fails; surrounding or internal ASCII whitespace around an otherwise even valid byte sequence does not.
- Truncated frames, invalid checksums where the selected public rule requires them, and selected-mode or direction mismatches fail; a complete matching fixture does not.
- JT808 blank lines and a trailing LF generate no record; a nonblank invalid line still generates a line-specific failure record.
- JT808 duplicate package index, mismatched declared count, and missing indexes do not merge; all continuous indexes from `1` to the shared declared total merge only within the defined group key.
- JT809/JT1078 internal newlines fail rather than becoming a batch. JT809 parameter values outside decimal `uint32`, including signed or fractional values, fail.
- Invalid UTF-8 and a lone UTF-16 surrogate fail in their relevant Hex direction; a valid multi-byte UTF-8 sequence succeeds.
- Ruiding and GPS51 cannot be selected. Structurally valid, unlisted messages are not incorrectly reported as malformed; they produce the explicit unsupported-body state.

## Verification layers and RED criteria

| Requirement | Acceptance criteria | Layer | RED evidence before implementation |
| --- | --- | --- | --- |
| REQ-001 | AC-001, AC-002 | Component | Catalog registration and independent mounted-tab-state tests fail. |
| REQ-002 | AC-003 to AC-005 | Unit and component | Navigation subtab, Launcher alias, and recoverable visibility tests fail. |
| REQ-003 | AC-006, AC-007 | Unit and component | Tests fail because parser state leaks to storage/IPC or survives an unmount. |
| REQ-004 | AC-008 to AC-011 | Unit and component | JT808 mode, version, private-option, and unsupported-body fixture tests fail. |
| REQ-005 | AC-012 to AC-016 | Unit and component | JT809 selectors, decimal parameter boundaries, frame-only encrypted state, and `0x0200` tests fail. |
| REQ-006 | AC-017 to AC-019 | Unit and component | JT1078 operation/direction and unsupported-body fixture tests fail. |
| REQ-007 | AC-020 to AC-022 | Unit and component | Hex direction, LF, formatting, example, and copy tests fail. |
| REQ-008 | AC-023 to AC-025 | Unit and component | CRLF/LF, original-line, subpackage, and single-packet tests fail. |
| REQ-009 | AC-026, AC-027 | Unit | Invalid boundary and valid counterexample tests fail. |
| REQ-010 | AC-028 to AC-030 | Component | Tree order/shape and deterministic plain-text copy tests fail. |
| REQ-011 | AC-031, AC-032 | Component | Tab action transition and clipboard success/failure tests fail. |
| REQ-012 | AC-033, AC-034 | Unit and behavior | Network-denied fixed-fixture suite fails before parser implementation exists. |
| REQ-013 | AC-035, AC-036 | Build, smoke, and change audit | Build, desktop smoke, obsolete visibility, and isolated-path audit fail. |

## Compatibility and rollback

The existing More Tools catalog, `ResolvedNavigationTarget`, App shell state, Launcher quick-tool list, visibility localStorage record, Toast provider, light/dark themes, and Tauri 2 desktop support contract remain compatible. `jttParserTab` is an optional navigation field, and the JT/T visibility field is additive with a default of `true`; existing records missing it remain valid.

No packet, result, JT809 parameter, database, Rust, command, service, or Tauri data is created. To roll back, remove the parser module and component, its tests, the optional navigation field, and its catalog, Launcher, presentation, and visibility registrations. Existing visibility records containing the now-obsolete field remain harmless because the reader only consumes recognized keys.
