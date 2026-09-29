# Agent Note: Gateway Terminal Sync Flags Model-Selection Drift as Pending

Status: implemented

English | [中文](2026-09-29-gateway-terminal-pending-model-drift.zh.md)

## Problem

The terminal panel's `pending_sync` flag only compared the persisted sync ledger: the default local key id and the local API address recorded by the last sync. It never looked at the gateway provider record that sync had written on the tool side. After an upstream change — a mapping's protocol, an `enabled` flip, a mapping auto-disable, or a different `default_model` — or after an app upgrade that changes the generated model list (such as the per-model `provider.npm` override added for OpenCode Responses-only models), the stored record could be stale while the badge stayed green "Synced". Users saw no signal that pressing `Sync` was required.

## Decision

`terminal_targets_from` (`src-tauri/src/ai_gateway/commands.rs`) now ORs the ledger comparison with a model-selection comparison for every synced target. The new private helper `terminal_model_selection_drifted(tool, stored, generated)` compares the stored tool-side gateway record against a freshly built `build_gateway_provider` payload for the same provider id, tool and base URL:

- `opencode`: deep equality of `tool_config.models` — the whole model list a sync writes, including entry shape and array order.
- `codex`: equality of the record's top-level `model` field, not `tool_config.model` (which holds `wire_api`), because that is where the sync writes codex's model.
- A compared key present on only one side counts as drifted.
- A payload build failure counts as drifted, so a fresh selection that cannot be built never reports synced.
- Key material is never compared: the comparison payload is built with an empty placeholder key because the app-store provider list redacts opencode keys. Key-id drift stays covered by the existing ledger comparison (`terminal_sync_pending`).

`pending_sync` for a synced target with a ledger record is therefore `terminal_sync_pending(record, default_key_id, base_url) || terminal_model_selection_drifted(...)`; without a ledger record it was and remains pending. The `synced` predicate, the ledger reuse rules and the response fields are unchanged, so `synced` can stay true while `pending_sync` is true.

## Alternatives considered

- Compare the whole generated provider record including `base_url` and `options.apiKey`: declined because key values are not available from the app-store list (opencode keys are redacted), and key-id and base-URL drift are already covered by the ledger comparison.
- Compare `tool_config.model` for `codex`: declined because the sync writes codex's model at the top level of the record while `tool_config` holds `wire_api`, so that path would never detect drift.
- Persist a model-list fingerprint in the sync ledger: declined because it would add a second stored copy to keep in version, while rebuilding the fresh payload from the current config is exact and leaves the ledger schema unchanged.
- Treat a payload build failure as not drifted: declined because the fresh selection then cannot be verified, and a false "Synced" would hide exactly the staleness this check exists to expose.

## Consequences

- Any upstream mapping, protocol, enabled or auto-disabled state or `default_model` change that alters the model selection, and any app upgrade that changes the generated entries (such as the per-model `provider.npm` override), now shows previously synced rows as `Pending sync` until the user syncs again; the row's single action button already reads `Sync` for a synced-but-pending target, so no frontend change was needed.
- The check is read-only: `ai_gateway_terminal_targets` builds the fresh payload in memory and never writes config, ledger or tool records.
- Verification: `cargo test --manifest-path src-tauri/Cargo.toml --lib terminal_targets_from` (exit 0, 9 passed) and `cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway::` (exit 0, 588 passed, 0 failed); the tests cover opencode model drift and the matching record, codex top-level `model` drift, and the in-sync fixture.
- Supersession: partial. [API Fusion Terminal Sync Writes an Independent Gateway Provider](../feature/2026-09-17-api-fusion-terminal-independent-provider.md) recorded that the terminal panel reads the backend-provided `pending_sync` instead of deriving pending state; that frontend contract and the independent-provider decision remain in force, while this record broadens the backend `pending_sync` computation behind that value. [API Gateway Resolves the Listening Port per Build Profile](../architecture/2026-09-22-gateway-build-profile-port.md) records cross-profile base-URL drift as an accepted pending case, which remains one of the two drift branches, and [Template Sync Best-Effort Refreshes Previously Synced Terminal Tools](../feature/2026-09-23-template-terminal-resync.md) reuses the unchanged `synced` predicate.
- `MEMORY.md`, `docs/USAGE.md` and the `ai-gateway-backend` entry of `.ai-workflow/index/navigation.json` with the regenerated `navigation.md` state the model-selection drift rule in the same change.
