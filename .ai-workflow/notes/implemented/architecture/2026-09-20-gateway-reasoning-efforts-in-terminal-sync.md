# Agent Note: Gateway Reasoning Efforts Sync to OpenCode Model Variants

Status: implemented

English | [中文](2026-09-20-gateway-reasoning-efforts-in-terminal-sync.zh.md)

## Problem

A provider mapping's `reasoning_efforts` list records the reasoning strengths that model advertises, and the operator maintains it in `ProviderDetailDialog`. Terminal sync ignored it: `build_gateway_provider` (`src-tauri/src/api_gateway/commands.rs`) wrote each opencode model entry as `{ "name": ... }` only, and `render_opencode` projected that `tool_config.models` map verbatim into `~/.config/opencode/opencode.json`. OpenCode therefore saw a custom model with no declared strengths and fell back to its own catalog or built-in inference for the model id, so the strengths offered for a model such as `deepseek-v4.1-flash` disagreed with the gateway's configured tiers. The earlier record stated the opposite on purpose ("never written into terminal-sync configuration"), which is exactly the behavior the operator reported as a defect.

## Decision

When the terminal sync builds the opencode gateway provider, every mapping whose `reasoning_efforts` is non-empty carries them into its model entry. `build_gateway_provider` writes `"reasoning": true` and a `variants` object keyed by each effort identifier whose value is `{ "reasoningEffort": "<effort>" }`; a mapping with no efforts keeps the previous `{ "name": ... }` shape, so an unconfigured model is unchanged. Effort identifiers are trimmed, blank ones are dropped and duplicates collapse through the object key. The mapping key stays `local_model`, the first duplicate still wins, and only the enabled rows of an active gateway are emitted exactly as before. Codex is untouched: its gateway record keeps its single `model` selection and gains no variants. `reasoning_efforts` is still absent from templates and never written by a template sync; only the terminal-sync output now reflects it.

## Alternatives considered

- Leave the terminal sync as it was and rely on OpenCode's built-in variants: declined because the tool then advertises strengths the gateway operator never configured, which is the reported inconsistency; the mapping list is the operator's source of truth.
- Write the tiers as a single model `options.reasoningEffort`: declined because one value pins a single strength instead of advertising the selectable set; OpenCode models strength selection as `variants`.
- Write `variants` without `reasoning: true`: declined because the flag marks the model as a reasoning model and drives the variant toggle in the TUI, so the variants would not surface for a plain custom model.
- Also emit variants for codex: declined because a codex gateway record selects exactly one model and expresses effort as a single `model_reasoning_effort`; a tier list has no place there, and the request named opencode.

## Consequences

- The opencode gateway provider written by `api_gateway_sync_terminal` now exposes exactly the mapping's configured strengths for the selected model; an empty list produces the old minimal entry, so an older build reads it unchanged.
- No persisted gateway schema, command signature or template shape changes: `reasoning_efforts` stays a mapping field with its serde default, and rollback simply drops the extra model keys on the next sync.
- `MEMORY.md` and the `api-gateway` / `api-gateway-backend` navigation entries now describe the synced variants; `navigation.md` was regenerated from the authoritative JSON.
- Supersession: partial. [API Gateway Provider Templates and Incremental Model Sync](2026-09-18-api-gateway-provider-templates.md) stated that mapping reasoning efforts are never written into terminal-sync configuration; that terminal-sync fact is corrected here, while its naming, template binding, ignored-model lifecycle, untouched-only merge and weekday price decisions remain in force. The remaining active records are unrelated.
