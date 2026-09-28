# Agent Note: OpenCode Responses-Only Models Carry the @ai-sdk/openai npm Override

Status: implemented

English | [中文](2026-09-28-opencode-responses-npm.zh.md)

## Problem

The synced opencode gateway provider uses the provider-level npm `@ai-sdk/openai-compatible` (`tool_config.npm` with `tool_config.options`), and that package's language model always POSTs `/chat/completions`. A local model the gateway can only serve through the Responses protocol — every enabled mapping row for it declares `responses` or belongs to a responses-protocol gateway — therefore failed through opencode: opencode POSTed `/chat/completions`, resolution found only responses rows, and the gateway answered `502` with `all providers unavailable: ... serves model '...' via /responses`. The gateway does no request-body conversion by its documented boundary, so the relay cannot bridge the two protocols; only the tool-side endpoint choice can.

## Decision

`build_gateway_provider` (`src-tauri/src/ai_gateway/commands.rs`) now decides per local model. Before writing an opencode model entry it evaluates two booleans across every enabled gateway with the new private helper `gateway_protocol_servable(gateways, local_model, protocol)`, which counts a protocol as servable only when `selection::resolve_model_for_protocol` returns `Serve(_)` for that gateway — the same authoritative resolution the request path uses, so disabled and auto-disabled rows never count, row-level protocol overrides apply, and the `default_model` fallback is honored. When the local model is not chat-servable but is responses-servable, the entry gains `"provider": { "npm": "@ai-sdk/openai" }` (new private constant `OPENCODE_RESPONSES_NPM`), the package whose bundled `languageModel()` resolves to the Responses API and POSTs `/responses` on the same `baseURL`. Any enabled gateway that can serve the model through Chat Completions keeps the existing entry shape, so mixed-protocol local models stay chat and the provider-level `@ai-sdk/openai-compatible` default is untouched. The gateway still performs no request-body conversion.

## Alternatives considered

- Convert chat requests to Responses inside the gateway: declined because it violates the documented no-conversion boundary; the relay forwards bytes unchanged and the endpoint choice belongs to the tool side.
- Hide responses-only local models from the synced opencode list: declined because it hides models the gateway can serve and other callers can already reach over `/responses`.
- Switch the whole opencode provider entry to `@ai-sdk/openai`: declined because chat-only models in the same mapping list would then POST `/responses` and break.

## Consequences

- A local model only Responses can serve is now usable through opencode, which calls `/responses` for that model entry; chat-only and mixed-protocol models keep their previous shape, so opencode's default `/chat/completions` path is unchanged for them. A mixed-protocol local model still goes through chat even when some gateway could serve it over Responses.
- Servability is evaluated per local model across all enabled gateways, never per contributing row: a disabled gateway, a user-disabled row or an auto-disabled row cannot make a model responses-only, while a model served by a gateway's `default_model` can count.
- The override lives only in the tool-side record written by terminal sync; no persisted gateway schema, command signature or template changes, so rollback simply drops the extra key on the next sync.
- Verification: `cargo test --manifest-path src-tauri/Cargo.toml --lib build_gateway_provider` (exit 0, 20 passed) and `cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway::` (exit 0, 574 passed) cover responses-only, row-level protocol override, mixed-protocol chat wins, disabled/auto-disabled rows and `default_model` servability.
- Supersession: partial. [Gateway Reasoning Efforts Sync to OpenCode Model Variants](../architecture/2026-09-20-gateway-reasoning-efforts-in-terminal-sync.md) recorded that a mapping without `reasoning_efforts` keeps the `{ "name": ... }` shape; that now holds only while an enabled gateway can serve the model through Chat Completions, since a responses-only entry also carries `provider.npm`. Its variants, first-duplicate-wins and enabled-row rules remain in force, and the remaining active terminal-sync records are unrelated.
- `MEMORY.md`, `docs/USAGE.md`, the `ai-gateway-backend` entry in `.ai-workflow/index/navigation.json` and the regenerated `navigation.md` state the per-model npm rule in the same change.
