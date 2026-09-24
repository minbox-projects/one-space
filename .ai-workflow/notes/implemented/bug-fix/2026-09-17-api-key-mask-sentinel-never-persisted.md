# Agent Note: New Local API Keys Never Persist the Mask Placeholder

Status: implemented

English | [中文](2026-09-17-api-key-mask-sentinel-never-persisted.zh.md)

## Problem

The local-key list renders `maskSecret(key.value)` instead of the stored secret, so the `value` field a client submits can hold either a real secret or the mask placeholder. Any create path that persisted the placeholder literally would make the relay authenticate with a publicly known string and present it in the list as the key's own secret; only the backend can turn "no value supplied" into real entropy, so the guarantee must hold in the backend no matter which client echoes the mask. The original fix honored both a blank value and the exact sentinel `"********"` as "no value supplied"; that sentinel contract has since been removed entirely.

## Decision

A name-only create never carries key material: the frontend no longer sends a mask, an all-whitespace value means "no value supplied", and `ai_gateway_upsert_key` replaces it with a backend-generated `sk-gateway-` secret carrying 128 bits of OS entropy (`src-tauri/src/ai_gateway/storage.rs`). When the id matches an existing key, a blank value preserves the stored secret and any other non-blank value replaces it. The `"********"` sentinel comparisons in `ai_gateway_upsert_provider` / `ai_gateway_upsert_key` and the frontend `AI_GATEWAY_KEY_MASK` constant are removed ([Gateway Migration Is Permanent and Version-Gated](../architecture/2026-09-24-version-gated-gateway-migration.md)); the whole-configuration `ai_gateway_save_config` path that normalized submitted keys is gone as well, and every write goes through the provider and key upserts. The frontend create flow lives in `src/components/AiGateway/LocalKeyDialog.tsx`, which collects only a label; `src/components/AiGateway/LocalKeyList.tsx` owns only the `Add key` button and the key list, and `src/components/AiGateway/index.tsx` holds the dialog's open state.

## Alternatives considered

- Store the submitted placeholder literally: declined because the sentinel is public, so the stored key would be a known string that the relay accepts and the UI presents as a real secret, breaking the rule that key material comes from backend-generated entropy.
- Keep honoring the exact sentinel `"********"` after removing the frontend constant: declined because blank already means "no value supplied" on both create and edit, and the constant was the only producer of the placeholder; the comparison would preserve a compatibility branch no supported caller can reach.
- Reject a new key that arrives with a blank value and ask the caller for a real secret: declined because the create interaction deliberately asks for a label only, and creating a key must still succeed in that single step.
- Always generate a value for every new key and ignore the submitted field entirely: declined because the command contract accepts an explicitly supplied key value, so discarding a real value would make an intentional caller's write silently ineffective.

## Consequences

- A new local key can only be created with a backend-generated `sk-gateway-` value carrying 128 bits of entropy; the literal `"********"` is no longer recognized, and no supported caller can send it.
- Editing an existing key keeps its behavior: a blank value preserves the stored secret and an explicit non-blank value replaces it. The former exact-string sentinel comparison no longer exists, so a client that still sends `"********"` would now be treated as supplying an explicit value rather than as the mask.
- `MEMORY.md`'s local-key standard now reads "a new key takes only a name and gets a backend-generated value, while an edit keeps the stored value when the field is blank"; this change updated that bullet in the same change.
- Regression coverage lives in the gateway storage test module, which creates a key without a value and edits an existing key with a blank value.
- Partial supersession: [Gateway Migration Is Permanent and Version-Gated](../architecture/2026-09-24-version-gated-gateway-migration.md) replaces only the sentinel comparison recorded above; the guarantees that a name-only create receives backend-generated entropy and that a blank edit preserves the stored secret stand, and [API Fusion Local Key Creation Uses a Name Dialog](../feature/2026-09-17-api-fusion-local-key-name-dialog.md) still records the local-key create flow.
