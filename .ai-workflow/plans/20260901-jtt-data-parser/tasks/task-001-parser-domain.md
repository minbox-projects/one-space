---
id: "task-001-parser-domain"
requirements: ["REQ-003", "REQ-004", "REQ-005", "REQ-006", "REQ-007", "REQ-008", "REQ-009", "REQ-010", "REQ-012"]
acceptance_criteria: ["AC-006", "AC-008", "AC-009", "AC-011", "AC-013", "AC-015", "AC-016", "AC-018", "AC-019", "AC-020", "AC-021", "AC-023", "AC-024", "AC-025", "AC-026", "AC-027", "AC-030", "AC-033", "AC-034"]
depends_on: []
surface: frontend
feature: "more-tools"
locator_read_order: ["src/App.tsx", "src/components/MoreToolsHub.tsx", "src/components/Launcher.tsx", "src/lib/navigation.ts", "src/lib/launcherToolVisibility.ts", "src/lib/moreToolPresentation.ts", "src/App.moreToolsNavigation.test.tsx", "src/components/MoreToolsHub.test.tsx", "src/components/Launcher.test.tsx", "src/lib/navigation.test.ts", "src/lib/launcherToolVisibility.test.ts"]
read_scope: ["MEMORY.md", ".ai-workflow/index/navigation.json", ".ai-workflow/index/navigation.md", "src/App.tsx", "src/components/MoreToolsHub.tsx", "src/components/Launcher.tsx", "src/lib/navigation.ts", "src/lib/launcherToolVisibility.ts", "src/lib/moreToolPresentation.ts", "src/App.moreToolsNavigation.test.tsx", "src/components/MoreToolsHub.test.tsx", "src/components/Launcher.test.tsx", "src/lib/navigation.test.ts", "src/lib/launcherToolVisibility.test.ts", "src/lib/jttDataParser/"]
new_module_directories: ["src/lib/jttDataParser/"]
write_scope: ["src/lib/jttDataParser/"]
test_commands: ["npm run test -- src/lib/jttDataParser/"]
---

# Task

## Objective

Independently implement the synchronous, front-end-only JT/T parser and converter domain for the frozen support matrix, including independently authored offline fixtures, Chinese result records, and deterministic indented plain-text result serialization.

## Implementation notes

- Implement the frozen JT808 five-mode frame, line, escape, checksum, package, `0x0801`, version, and unsupported-body behavior. Do not add Ruiding, GPS51, unlisted message-body decoders, or invented body fields.
- Implement the frozen JT809 2011/2019 frame scope, the 2019 `0x0200` bridge, decimal `uint32` validation for `M1`, `IA1`, and `IC1`, and encrypted-body state without claiming unsupported decryption.
- Implement JT1078 linkage operation and direction behavior only for `0x9101`, `0x9102`, `0x9205`, and `0x9206`; retain diagnosable frame-level output for unlisted bodies.
- Implement line-preserving Hex/UTF-8 conversion with the frozen ASCII-whitespace, LF/CRLF, empty-line, odd-length, invalid UTF-8, and isolated-surrogate rules.
- Keep all parser work local and synchronous. Do not invoke Tauri, use network APIs, persist packet data, or introduce an external dependency.
- Use independently derived fixtures and result expectations. Do not introduce JTTools source, tests, wording, samples, branding, attribution, links, or license text.

## Negative cases

- Reject empty input, non-Hex, odd Hex length, malformed/truncated frames, required checksum failures, JT809 parameter values outside decimal `uint32`, internal newlines in JT809/JT1078, invalid UTF-8, isolated surrogates, and mismatched JT1078 direction.
- Preserve JT808 line positions; discard only blank or ASCII-whitespace-only lines. Do not merge duplicate, conflicting, or incomplete JT808 subpackages.
- Structurally valid bodies outside frozen coverage must emit frame fields, message ID, raw body Hex, and an unsupported-body state rather than a parser failure or fabricated field tree.

## Test evidence

- `npm run test -- src/lib/jttDataParser/` proves offline fixtures for every frozen support-matrix row, supported output, unsupported body, package state, invalid boundary, and legal counterexample.
- The test environment must deny `fetch`, `XMLHttpRequest`, and parser-related Tauri invocation so a passing suite demonstrates AC-006 and AC-034 rather than relying on external services.

## Completion definition

- All assigned REQ and AC identifiers are covered by pure domain code and independently authored fixtures.
- Changes remain within `src/lib/jttDataParser/`.
- The assigned test command passes with network and parser IPC denied.
