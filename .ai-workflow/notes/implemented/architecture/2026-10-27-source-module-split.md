# Agent Note: Source Modules Use Facades over Responsibility Submodules

Status: implemented

English | [中文](2026-10-27-source-module-split.zh.md)

## Problem

Several non-test source files had grown past 1,200 lines by accumulating unrelated responsibilities: React state and markup, command wrappers, protocol helpers, persistence, runtime orchestration and inline test seams shared one file. That size made ownership hard to see, made navigation indexes point at monolithic entry points and forced reviewers to reason about many concerns at once. A behavior-preserving split had to solve this without changing anything an importer, a test, a Tauri command caller, a persisted file or a translation consumer can observe, so a proposed decomposition that required a logic, signature, condition, hook-order or serialization change had to stop instead of being forced.

## Decision

Every original non-test source file that exceeded the 1,200-line threshold is now a facade or container at its original path and at or below 1,200 lines. `src/i18n.ts` and `src/lib/aiGateway.ts` became directory index facades at `src/i18n/index.ts` and `src/lib/aiGateway/index.ts`; every other original path remains as a facade or container, including the case-safe `src/components/SshTunnels.tsx` beside `src/components/sshTunnels/`.

Rust targets keep the existing `foo.rs` plus `foo/` form. The root file declares its submodules, re-exports every public, `pub(crate)` and `pub(in crate::ai_gateway)` path that consumers or inline `#[cfg(test)]` modules reach, and leaves generated command macros and command registration unchanged. Responsibility submodules stay under the same domain and may themselves be directories such as `commands/`, `model_pools/`, `openai_conversion/`, `runtime_http/`, `selection/`, `templates/`, `types_config/` and `usage_log/`.

Frontend pure modules use a directory index over acyclic domain files. The i18n index merges locale domains by feature prefix into the same English and Chinese translation resources, and the AI Gateway index re-exports types, commands, model pools, providers, prices, usage and templates so consumers still import `@/lib/aiGateway`. Large React components keep a container at the original path and compose module-level presentational components and hooks from the same domain, with unchanged hook order and effect dependencies.

Exactly one new file exceeds 1,200 lines: `src-tauri/src/ai_gateway/runtime_http/streaming.rs` is 1,300 lines and contains a byte-identical `attempt_streaming_inner`, moved without a statement change. Every other new file is at or below 1,200 lines.

The no-behavior-change rule is part of the architecture: no business logic, condition, calculation, parameter, return shape, signature, serialization attribute, command name or registration, event name or payload, persistence format, schema version, i18n key or value, UI layout, copy or test was changed. When a split would need any of those changes, the work stops and reports the boundary rather than rewriting semantics.

## Alternatives considered

- Keep the large files and divide them with section comments: declined because comments do not reduce file size, ownership ambiguity or review surface.
- Move public symbols to new module paths and update importers: declined because every existing specifier, consumer and test must keep resolving without edits.
- Rebuild each entry as a new barrel with different symbol identities: declined because it can change module identity, generated command macros and inline test seams.
- Split `attempt_streaming_inner` further to satisfy the 1,200-line cap: declined because it would require statement-level changes, so one 1,300-line exception with a byte-identical body is recorded instead.
- Introduce a shared generic split engine or merge the Skills and Subagents implementations: declined because it would change behavior and exceeds the structural goal.
- Rename or relocate `SshTunnels.tsx` by letter case: declined because APFS is case-insensitive and the facade must stay beside the existing `sshTunnels/` directory.
- Add or modify tests to make the decomposition easier to verify: declined because the existing suites must remain the unchanged behavior gate.

## Consequences

- Every original entry path resolves to the same public surface, so existing imports, Tauri command registration, event contracts, persisted files, schema versions and translations remain compatible.
- New implementation files are capped at 1,200 lines, with the streaming file as the only 1,300-line exception; the facade or container at each original path stays at or below 1,200 lines.
- Navigation indexes now point at the directory index entry points and at the domain module files that declare public symbols, instead of the retired monolithic paths.
- No test file was added, deleted or modified, and the existing frontend and Rust suites remain the behavior gate for the split.
- Each structural step can be reverted independently because consumers depend on facades rather than on another step's new internal modules.
- Verification: `ai-workflow context validate` and `ai-workflow notes validate` pass for this documentation state, and the documented paths and symbols match the delivered tree.
- No active note is fully or partially superseded. Existing notes about feature behavior, gateway routing, persistence and performance remain current because this record changes only module boundaries and preserves their recorded contracts.
