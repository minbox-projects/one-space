# Agent Note: AI Workflow Model Switcher Adds Discovered Free OpenCode Models to Column Candidates

Status: implemented

English | [中文](2026-10-15-ai-workflow-free-model-sources.zh.md)

## Problem

`ai_workflow_get_model_sources` built the OpenCode column candidates only from models explicitly declared under `provider.<name>.models` in `~/.config/opencode/opencode.json`. Operators therefore had to declare every model they wanted to switch to, even though OpenCode already publishes its current model catalog locally, so the free models that OpenCode discovers and refreshes stayed invisible to the switcher until a user copied each id into `opencode.json`. The OpenCode capability derivation already read that local catalog for `reasoning_options`, but the candidate list itself never used it.

## Decision

The OpenCode column candidates are the union of the explicitly declared models and the dynamically discovered current free models, deduplicated and sorted.

- The dynamic source is the `opencode` and `opencode-go` providers in `~/.cache/opencode/models.json`.
- A catalog model counts as free when it carries a `cost` object whose `input` and `output` are both `0` and its `status` is not `"deprecated"`.
- Capability tiers for a dynamic candidate are resolved through `opencode_catalog_capability` from the catalog `reasoning_options` entries with `type: "effort"`, reusing the same catalog read as the existing capability fallback.
- An explicitly declared model's `variants` capability keeps precedence and is never overwritten by a catalog-derived capability.
- A missing catalog degrades silently to declared-only candidates: no dynamic entries and no new error or warning. A present but unreadable or malformed catalog keeps the existing `capability_warning` behavior.
- No model id is hardcoded and no network request is made; the switcher reads only the local OpenCode snapshot.

## Alternatives considered

- Hardcode a curated list of free model ids: declined because the catalog changes, so a snapshot list goes stale and creates maintenance that contradicts the zero-maintenance goal.
- Fetch the model catalog from the network: declined because it adds latency and network dependence, would leave the switcher unusable offline and matches the existing local-only rationale.
- Treat a model as free by a name heuristic such as an id fragment: declined because a name does not prove billing, while the declared `cost` values are authoritative.
- Keep candidates declared-only and require users to declare free models manually: declined because OpenCode already maintains the local catalog the capability path reads, so requiring manual duplication defeats the discovery.
- Surface a new error when the catalog is missing: declined because a missing local catalog is a normal state, and a silent declared-only result keeps the column usable.

## Consequences

- The OpenCode column offers both explicitly declared models and the currently free catalog models, deduplicated and sorted, while declared entries always remain available.
- A declared model's explicit capability is authoritative and is never replaced by a catalog-derived effort list.
- Missing catalog data is silent; malformed catalog data keeps the existing `capability_warning`.
- The free list is a dynamic local snapshot: it tracks the OpenCode catalog as it refreshes, so a model can appear or disappear between reads and no model id is promised to stay free.
- The `ai-workflow-model-switcher` and `ai-workflow-backend` entries in `.ai-workflow/index/navigation.json`, the generated `.ai-workflow/index/navigation.md` and the AI Workflow standard in `MEMORY.md` record the delivered contract.
- Partial supersession: this record partially supersedes the OpenCode candidate-source enumeration of [AI Workflow Model Switcher Delivers 9-by-3 Matrix with Backend Profile Commands](2026-09-22-ai-workflow-model-switcher.md), which is retained and cross-linked; its 9-by-3 matrix, per-column degradation, profile management and Save/Activate rationale still stand. [AI Workflow Model Switcher Derives Reasoning Efforts from Declared Local Model Capabilities](../bug-fix/2026-10-15-ai-workflow-model-reasoning-efforts.md) records the capability derivation and is unchanged.
