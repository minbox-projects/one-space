# Agent Note: Model Pools Bind Supported Native Reasoning Levels Directly in the Candidate Editor

Status: implemented

English | [中文](2026-10-16-model-pool-reasoning-bindings.zh.md)

## Problem

The pool-local reasoning editor required the operator to declare a candidate's native reasoning profile and its declared value set before any public preference could be mapped to a native level, so the same native values were maintained twice and a candidate could not be configured by simply choosing among the levels its source already exposes. The user requested a directly scoped simplification: bind each supported native effort to a unified model-pool effort for the selected candidate and show the unified tiers in the model list, without a new persisted format, a backend change or any effect on existing pool-only routing, parsing, isolation and default guards.

## Decision

- The candidate editor's main panel binds each canonical pool preference (`low`, `medium`, `high`, `xhigh`, explicit `none`) to one native level through a plain native-value select. Option labels are the plain native values; the `enum:<value>` spelling stays an internal option value only and is never displayed.
- The native options of one candidate are the union of the exact source mapping's declared `reasoning_efforts` — resolved by provider id plus the trimmed original local/upstream pair plus the effective protocol — the candidate's own declared values and its stored enum action values. Every value is trimmed and de-duplicated without rewriting its case, and no sibling model, other provider or name-based profile guess contributes.
- A positive preference never offers the explicit disable action, and `none` never offers an enum or a budget: `none` offers the explicit disable when the candidate's union of native levels already contains `none` or the advanced `supports_disable` toggle is set.
- Only an explicit selection creates a pool-local declaration. When the profile is still empty, the first binding selects the candidate protocol's default adapter (`openai_effort` for Chat Completions and Responses, `anthropic_effort_inherit` for Anthropic Messages); an existing non-empty profile always stays exactly the operator's own choice. A newly bound native value is appended to the prior declared values only when it is not already present.
- The native profile, declared values, budget range and `supports_disable` fields move behind a default-closed "Advanced declaration" `<details>`, while the outer per-candidate panel keeps its existing default-collapsed state. An untouched candidate still emits no `reasoning` key.
- A manual-budget binding keeps the manual-budget profile and offers the budget action with its own native token count, and a switch profile keeps its no-intensity-gradient note; both retain the correct native actions.
- The model list already advertises the canonical pool reasoning catalog and ordinary provider native badges, so no product change was needed there; a new regression confirms that binding canonical `high` to native `max` shows `high` on the pool row and `max` on the ordinary provider row.
- No DTO, schema, backend, public exported symbol or path changes.

## Alternatives considered

- Keep all original fields required instead of a binding-first editor: declined because that retains the redundant profile and value maintenance the direct request removes; the fields stay available behind the advanced disclosure rather than being mandatory.
- Replace the persisted pool-local declaration format or add a backend change: declined because it adds extra steps and scope without a user-visible benefit; the existing pool-local `reasoning` payload and its backend validation are reused unchanged.

## Consequences

- Scope: only the pool candidate editor and the pool-local declaration it derives change. The source provider configuration and the ordinary provider editor are untouched, and the new regression keeps the pool row's canonical tiers separate from the ordinary row's native identifiers.
- Existing behavior retained: manual-budget per-mapping tokens, range validation, draft retention on a rejected save, the non-overwrite of unsaved edits on a runtime refresh, and every routing, parser, isolation and default guard of the pool-only reasoning policy stay in force.
- Missing metadata: a candidate whose exact source no longer declares `reasoning_efforts` still offers only its advanced-declared and stored enum values, an empty option set leaves a positive preference explicitly unmapped, and no source metadata value is promoted into a mapping.
- No live-provider claim: the editor derives the declaration deterministically from the current configuration; it does not prove that a real provider obeys the bound native level, and no live provider check was run.
- Partial supersession: this record partially supersedes [Model Pools Own a Bounded Pool-Local Reasoning Policy](../feature/2026-10-15-pool-only-reasoning.md) only for the pool editor's UI route and its default-adapter explicit-selection behavior — the candidate editor now binds canonical preferences in its main panel with the profile, values, range and disable fields behind a default-closed advanced disclosure — while every routing, parser, isolation and default guard in that record remains in force.
- Partial supersession of this record: this record is partially superseded by [Model Pool Candidate Editor Removes the Advanced Declaration Parameters](2026-10-16-model-pool-bindings-only-editor.md) only for its default-closed advanced declaration disclosure and its per-effort budget editing, both removed there as prior decisions; the binding-first main panel, its default-adapter explicit-selection behavior and every routing, parser, isolation and default guard remain in force.
