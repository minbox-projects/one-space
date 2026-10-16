# Agent Note: Model Pool Candidate Editor Removes the Advanced Declaration Parameters

Status: implemented

English | [中文](2026-10-16-model-pool-bindings-only-editor.zh.md)

## Problem

After the candidate editor became binding-first, the operator still saw an "Advanced declaration" disclosure that configured the native profile, the declared value set, the budget range and the `supports_disable` toggle, plus a per-effort numeric budget input. Those controls duplicated the native reasoning metadata the editor already derives and the stored declaration already carries, and a manual-budget candidate could still be rebound to a typed token count its source does not expose. The user requested a directly scoped follow-up: remove every advanced declaration parameter and its interface configuration item from the pool candidate editor.

## Decision

- The candidate editor's main panel keeps only the five canonical binding selects (`low`, `medium`, `high`, `xhigh`, explicit `none`) over this candidate's native levels; the default-closed "Advanced declaration" `<details>` and its native profile, declared value set, budget range and `supports_disable` controls are removed, as is the numeric per-effort budget input. The local reasoning draft stores only the operator's chosen bindings.
- Derivation keeps the immutable stored `candidate.reasoning` as a read-only base: a first binding on an empty profile selects the protocol default adapter (`openai_effort` for Chat Completions and Responses, `anthropic_effort_inherit` for Anthropic Messages), a bound native enum value is appended to `values` only when absent, and binding `none` to an explicit disable sets `supports_disable`. An untouched new candidate still emits no `reasoning` key and clearing every new binding leaves no orphaned declaration.
- A manual-budget candidate offers only its original distinct stored native budget actions as rebind targets, labelled `Budget · <tokens>` with option value `budget:<tokens>`; there is no numeric or range edit. A switch profile offers only `enabled` and keeps its read-only no-intensity-gradient help.
- A candidate whose exact source declares no native reasoning levels shows a hint pointing to the provider model reasoning tier configuration instead of advanced instructions.
- No DTO, schema, API, backend, module, public exported symbol or path changes.

## Alternatives considered

- Keep the advanced parameters behind the existing collapsed disclosure instead of removing them: declined because the direct request is to remove every advanced parameter and its interface item, and the derived declaration already covers what those controls configured.
- Replace the persisted pool-local declaration format or change the backend to accept the editor's derived payload shape: declined because the existing pool-local `reasoning` payload and its validation already support the derived declaration, so a format or backend change adds scope without a user-visible benefit.

## Consequences

- Scope: only the pool candidate editor changes; the source provider configuration, the ordinary provider editor and the pool-only routing, parsing, isolation and default guards are untouched.
- Retained behavior: the binding-first main panel, static coverage, the immutable stored metadata used for save and for a legitimate pre-existing manual-budget or switch choice, the draft retention on a rejected save, the non-overwrite of unsaved edits on a runtime refresh and every backend validation guarantee stay in force.
- Source metadata: a newly empty native level set at the source is configured through the provider model reasoning tier setting, not through the pool editor; the editor never promotes a source metadata value into a mapping.
- No live-provider claim: the derivation is deterministic from the current configuration and does not prove that a real provider obeys a bound native level; no live provider check was run.
- Partial supersession: this record partially supersedes [Model Pools Bind Supported Native Reasoning Levels Directly in the Candidate Editor](2026-10-16-model-pool-reasoning-bindings.md) only for its default-closed advanced declaration disclosure and its per-effort budget editing, both removed here; that record's binding-first main panel and every routing, parser, isolation and default guard remain in force.
