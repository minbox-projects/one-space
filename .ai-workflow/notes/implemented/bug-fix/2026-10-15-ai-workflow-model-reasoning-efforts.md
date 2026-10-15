# Agent Note: AI Workflow Model Switcher Derives Reasoning Efforts from Declared Local Model Capabilities

Status: implemented

English | [中文](2026-10-15-ai-workflow-model-reasoning-efforts.zh.md)

## Problem

The AI Workflow model switcher applied one fixed six-value reasoning-effort list (`low`, `medium`, `high`, `xhigh`, `max`, `ultra`) to every cell of the 9-by-3 role-by-host matrix. It therefore presented efforts that a host or model does not support, and `ai_workflow_save_profile` could persist a schema-valid but unsupported effort because it validated only the profile schema. The switcher had no per-model local capability awareness, no explicit manual path for models whose capabilities cannot be determined locally, and no way to signal that discovered values fall outside the profile schema. The upstream AI Workflow profile schema keeps the existing six-value effort enum, so a discovered value outside it cannot be written to a profile.

## Decision

The switcher derives reasoning efforts from explicitly declared local model capabilities while keeping the six-value profile schema as the writable boundary.

Backend model-source discovery (`ai_workflow_get_model_sources`) adds an additive `model_capabilities` map keyed by the exact host-local model id, plus an independent `capability_warning` kept separate from the column `error`. The public `ModelCapability` type carries `efforts: Vec<String>` and an optional `default_effort`; an omitted map or entry means the exact model's capability is unknown, an explicit empty entry means the capability is known to have no writable values, and an empty `model_capabilities` map is omitted from serialization.

- Codex reads `~/.codex/models_cache.json`, matching `models[].slug` and collecting non-empty `supported_reasoning_levels[].effort` together with the declared `default_reasoning_level`. An absent slug or empty effort metadata is unknown.
- OpenCode reads explicit `~/.config/opencode/opencode.json` model `variants` whose values declare a string `reasoningEffort`, excluding `disabled: true`. Effort values are read rather than variant names, and explicit declarations take precedence over a cached catalog, including when every declared effort variant is disabled (a known empty list that never falls back to the cache). Without such declarations, exact `provider/model` matches in `~/.cache/opencode/models.json` may supply `reasoning_options` entries with `type: "effort"` and string `values`. Full composite ids including slashes are preserved.
- Claude and any custom model without an explicit applicable source are unknown. A scalar configured effort is a selected default, not a capability list.
- Missing files are normal; a parse or read failure degrades that source to unknown with a `capability_warning` while keeping model-name candidates and healthy columns usable, and never exposes configuration contents or credentials.

The typed frontend domain library `src/lib/aiWorkflowProfiles.ts` exports `ModelCapability`, `ColumnModelSource`, `isKnownEffort`, `getModelCapability`, `resolveWritableEfforts`, `resolveExcludedEfforts` and `normalizeEffortForModel`.

- Known models offer only the intersection of the declared list with the profile enum, deduplicated in profile enum order (`resolveWritableEfforts`). Discovered values the schema excludes, such as `none` or `minimal`, are shown with a schema-limit explanation (`resolveExcludedEfforts`). An empty writable intersection disables the effort control, explains the incompatibility and makes a populated cell invalid for Save.
- Unknown models use manual input with a visible unknown-capability hint, validated against the six profile values (`isKnownEffort`).
- Every model-edit path — single-cell model choice, typed model entry, row fill and column fill — shares `normalizeEffortForModel`: keep a currently writable effort, else the declared default, else `medium` when writable, else the first writable effort. Unknown models keep the entered effort, and a newly populated unknown cell starts at `medium` without claiming support. A known model with no writable values has no automatic default and stays invalid.
- Loading preserves the stored matrix and the dirty baseline, including invalid values; Save blocks while any configured cell is invalid.
- Batch effort targets are the currently populated known cells, and the menu offers the intersection of every target's writable efforts. Unknown cells are skipped with a visible count, missing cells are never created, and a disjoint intersection, a known empty set or no target disables Apply with a message. A model or profile change recomputes the current targets and intersection and clears a stale selection, so a superseded effort cannot be applied.

Backend Save (`ai_workflow_save_profile`) keeps the existing six-value schema validation and, after it passes and before the atomic write, independently reloads the local capabilities through the same home resolution and rejects a known model's declared-unsupported effort even when it is schema-valid, returning an error that names the role, host and model. Both the profile and the active-profile configuration are left unchanged, and an unknown model's schema-valid manual value remains saveable. Save stays separate from activation.

All added hints and validation messages ship in English and Chinese in `src/i18n.ts` with matching `en_keys.txt` and `zh_keys.txt` entries: `aiWorkflow.effortSchemaExcluded`, `aiWorkflow.effortIncompatibility`, `aiWorkflow.effortUnknownHint`, `aiWorkflow.effortInvalid`, `aiWorkflow.batchEffortSkippedUnknown`, `aiWorkflow.batchEffortNoCommon` and `aiWorkflow.batchEffortNoTargets`.

Delivered behavior is covered by fixture-backed public Rust model-source and Save tests in `src-tauri/src/ai_workflow_profiles/tests.rs` (`cargo test --manifest-path src-tauri/Cargo.toml --lib ai_workflow_profiles`, commit `9a1f6fdf7ece5977b79502cfaebc65d268e7d957`, exited 0 with 35 passing, including the 26 retained cases) and by React Testing Library editor, model-change, batch and Save interactions (`npm test -- src/components/AiWorkflowModelSwitcher/AiWorkflowModelSwitcher.test.tsx src/lib/aiWorkflowProfiles.test.ts src/i18n.test.ts`, commit `faa50667fcc6f7e329196028976112cebdabb5fa`, exited 0 with 151 passing). `npm run build` and the scoped `npx eslint` command exited 0. No live provider probe, network catalog refresh or CLI installation was performed.

## Alternatives considered

- Keep the fixed six-value selector without local capability awareness: declined because it presented efforts a host or model does not support and could persist a schema-valid but unsupported effort; the required behavior is that known models offer only supported values and the backend rejects a known-unsupported effort before writing.
- Infer support from model-name substring heuristics: declined because a similar name does not prove support, and `provider-a/deepseek/r1` and `provider-b/deepseek/r1` are separate identities.
- Live provider probes, a network catalog refresh or CLI installation: declined because they add latency, network dependence and credential exposure; reading local declared snapshots keeps the tool offline and leaves credentials untouched.
- Extend the upstream AI Workflow profile schema, for example to add `none` or `minimal`: declined because the profile format stays at version `1.0.0` with the existing six values; discovered non-enum values are surfaced as schema-excluded and never written to profile YAML.
- Add capability override settings, dependencies, migrations or a generalized model catalog service: declined as unnecessary for locally declared snapshots.

## Consequences

- Known models offer exactly the declared-and-writable efforts and explain schema-excluded values; unknown models use explicit manual entry; a known model with no writable values is visibly invalid and blocked for Save.
- The backend remains the single authority for profile YAML: it re-reads current local declarations independently of the frontend snapshot and refuses a known-unsupported effort atomically, leaving the profile and active-profile configuration unchanged. A declaration change between edit and Save can therefore return a refusal instead of persisting an invalid value.
- Profile CRUD, the Save/Activate separation, the installed-agent snapshot, provider projections and model-name candidate semantics are unchanged; the IPC change is additive, an omitted capability map is treated as unknown, and profile YAML stays at `1.0.0`.
- Local cache and configuration declarations are snapshots with no live-freshness claim, and a manual value on an unknown model is not evidence that the model supports it.
- The `ai-workflow-model-switcher` and `ai-workflow-backend` feature entries in `.ai-workflow/index/navigation.json`, the generated `.ai-workflow/index/navigation.md` and the AI Workflow domain standard in `MEMORY.md` record the delivered capability contract.
- Partial supersession: this record partially supersedes the static six-value reasoning-selector decision of [AI Workflow Model Switcher Delivers 9-by-3 Matrix with Backend Profile Commands](../feature/2026-09-22-ai-workflow-model-switcher.md), which is retained and cross-linked; its 9-by-3 matrix, model-source, management and Save/Activate rationale still stand. [AI Workflow Profile Save and Activation Are Separate Actions](../feature/2026-09-23-ai-workflow-profile-save-activation.md) records the independent Save/Activate decision and is unchanged.
