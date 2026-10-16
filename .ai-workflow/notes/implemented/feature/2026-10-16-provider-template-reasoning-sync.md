# Agent Note: Provider Template Reasoning Capability Sync

Status: implemented

English | [中文](2026-10-16-provider-template-reasoning-sync.zh.md)

## Problem

- The CommandCode provider template fetched only the ordinary model list `https://api.commandcode.ai/provider/v1/models`, which carries membership, names and endpoint declarations but no reasoning-effort metadata, so a sync kept whatever template capabilities already existed and every derived mapping's `reasoning_efforts` stayed a manual value.
- CommandCode also publishes an official client catalog at `https://api.commandcode.ai/provider/v1/models?view=client` that declares `reasoningEfforts` (a string array or `null`) and no endpoints, and the operator wanted those declared capabilities refreshed together with the model list through the existing manual and scheduled flow, without overwriting independent operator edits or adding a persisted field, scheduler or UI.

## Decision

- The canonical source gate lives in `src-tauri/src/ai_gateway/templates.rs`: only template id `commandcode` with a trimmed `models_url` exactly `https://api.commandcode.ai/provider/v1/models` triggers a second credential-free request to the client catalog through the existing 15-second `reqwest` client; every other URL, including a CommandCode template with a different configured URL, is fetched once as configured with no extra official lookup.
- Both requests, the client-schema validation and the exact-id join are inside the `fetch_template_models_with` seam, which returns one enriched ordinary-list JSON body; `commands.rs` and the existing call signatures are unchanged.
- The ordinary list stays the sole authority for model membership, names and endpoint protocols; capabilities are joined by exact upstream id, a client-only or hidden catalog entry never adds a model, and the client catalog is never used to infer protocols.
- Object entries may declare `reasoningEfforts` (winning over a coexisting `reasoning_efforts`): an array is trimmed, blank-dropped and deduplicated in source order, an explicit `null` or `[]` clears the set, and an absent field or an ordinary id missing from the catalog preserves the previous value; any other declared shape fails the whole sync.
- CommandCode joins only the Provider API five-value set `low`, `medium`, `high`, `xhigh` and `max`; the CLI control `off` is never exported as an upstream effort. Other templates keep their explicitly declared tokens verbatim.
- Every existing matching mapping whose normalized set (trim, blank-drop, dedup, sort) equals the previous template model's set follows the new set, including each alias of one upstream model, user-disabled and auto-disabled rows and existing mappings of a disabled template model; an independently customized set is preserved, enablement and health flags never change, ignored models and disabled template models never create a mapping, and new or restored mappings copy the current capabilities through their existing paths.
- Both responses are fetched and parsed before any publication, and a request failure, an unsupported client `schemaVersion`, an invalid catalog `data` shape, a duplicate or missing client id, a malformed declared capability or a persistence failure leaves the template, the derived providers and `synced_at` unchanged and reports through the existing sync error surface.

## Alternatives considered

- Infer capabilities from model names, other providers or third-party catalogs, or scrape the CLI bundle at runtime: declined because that is a guess rather than explicit source metadata and contradicts the project's no-implicit-guessing boundary.
- Add a persisted capability field, a snapshot migration or a read-time normalization path: declined because `ProviderTemplateModel.reasoning_efforts` already exists and the sync must write explicit values only, keeping schema 3 unchanged.
- Unconditionally overwrite every derived mapping's efforts with the template set: declined because it would destroy the independent operator edits the change requires to survive.
- Translate the CLI control `off` into an upstream effort: declined because the Provider API accepts only five values and `off` is a CLI-only control that must not be exported.
- Treat an absent capability field or a missing supplemental id as an instruction to clear: declined because absence must preserve the previous value and only an explicit `null` or `[]` clears.

## Consequences

- Delivered code is `src-tauri/src/ai_gateway/templates.rs`, the `ModelMapping.reasoning_efforts` comment in `src-tauri/src/ai_gateway/types_config.rs` and fourteen new tests in `src-tauri/src/ai_gateway/tests/templates.rs`; `commands.rs` is unchanged.
- An existing stored canonical CommandCode template gains the declared capabilities on its next manual or scheduled sync without a template reset or configuration migration, a generic metadata-less source preserves previous values, and disabled provider, mapping and health states are never changed.
- Verification: `cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway::tests::templates` exited 0 with 86 passed, `...ai_gateway::tests::auto_refresh` exited 0 with 7 passed, and `...--lib ai_gateway` exited 0 with 966 passed plus the two pre-existing ignored performance cases; `cargo check --manifest-path src-tauri/Cargo.toml --tests` exited 0.
- `MEMORY.md`, the authoritative `.ai-workflow/index/navigation.json` (`ai-gateway-backend`) and the regenerated `navigation.md` record the explicit metadata source, the absent-versus-empty semantics, the inherited-ownership rule and the failure boundary in the same change.
- Supersession: partial. This record partially supersedes [Gateway Reasoning Efforts Sync to OpenCode Model Variants](../architecture/2026-09-20-gateway-reasoning-efforts-in-terminal-sync.md) for reasoning-capability ownership and source only, because a template sync now writes `reasoning_efforts` into inherited mappings; that record's variant-projection decision remains in force. It supersedes none of [Provider Templates Refresh Automatically on a Persisted Interval](../feature/2026-09-23-template-auto-refresh.md), [Template Auto Refresh Runs One Batch at App Start](../bug-fix/2026-09-29-template-autorefresh-startup-batch.md) and [Core Workflows Cleanup and Optimization](../architecture/2026-10-09-core-workflows-cleanup-and-optimization.md), whose scheduler and core-workflow decisions remain valid because this is explicit source metadata synchronization rather than ordinary-read or write-time inference.
