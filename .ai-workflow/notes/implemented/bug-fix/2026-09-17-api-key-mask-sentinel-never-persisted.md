# Agent Note: New Local API Keys Never Persist the Mask Placeholder

Status: implemented

English | [中文](2026-09-17-api-key-mask-sentinel-never-persisted.zh.md)

## Problem

The local-key list renders `maskSecret(key.value)` instead of the stored secret, and the frontend uses `API_FUSION_KEY_MASK` (`"********"`, `src/lib/apiFusion.ts`) as the sentinel that means "keep the stored value". Because that sentinel travels in the same `value` field as a real secret, any write path that creates a key could persist it literally: a new key created through `api_fusion_save_config` with `value: "********"` or `value: ""` was stored exactly as submitted, so the relay would authenticate with a publicly known placeholder and the key list would present it as its own secret. For a new key both a blank value and the placeholder mean "no value supplied", and only the backend can turn that into real entropy, so the guarantee must hold in the backend no matter which client echoes the mask.

## Decision

Both write paths that can create a local key treat an all-whitespace value or the exact sentinel `"********"` as "no value supplied" and replace it with `new_key_value()` — `sk-fusion-` plus a 128-bit `uuid::Uuid::new_v4().simple()` hex suffix (`src-tauri/src/api_fusion/storage.rs`). `api_fusion_upsert_key` applies this in its new-key branch, where the id is empty or unknown; when the id matches an existing key, a blank or sentinel value preserves the stored value and any other non-empty value replaces it. `api_fusion_save_config` normalizes the submitted keys the same way: a matched id keeps the stored value, and an unmatched (new) id with a blank or sentinel value gets a generated secret. The frontend create flow lives in `src/components/ApiFusion/LocalKeyDialog.tsx`, which collects only a label and submits `value: ""`; `src/components/ApiFusion/LocalKeyList.tsx` owns only the `Add key` button and the key list, and `src/components/ApiFusion/index.tsx` holds the dialog's open state. `API_FUSION_KEY_MASK` remains the "preserve the stored value" signal the frontend may send for existing records.

## Alternatives considered

- Store the submitted placeholder literally: declined because the sentinel is public, so the stored key would be a known string that the relay accepts and the UI presents as a real secret, breaking the rule that key material comes from backend-generated entropy.
- Let the frontend send an empty value and keep the backend unaware of the sentinel: declined because any caller can echo the mask, and only the backend write path can keep the literal out of `api_fusion.json`.
- Reject a new key that arrives with a blank or sentinel value and ask the caller for a real secret: declined because the create interaction deliberately asks for a label only, and creating a key must still succeed in that single step.
- Always generate a value for every new key and ignore the submitted field entirely: declined because the command contract accepts an explicitly supplied key value, so discarding a real value would make an intentional caller's write silently ineffective.

## Consequences

- A new local key can only be created with an `sk-fusion-` value carrying 128 bits of entropy; the literal `********` can no longer become the stored secret through `api_fusion_upsert_key` or `api_fusion_save_config`.
- Editing an existing key is unchanged: a blank or sentinel value preserves the stored secret and an explicit non-empty value replaces it. The sentinel is compared for exact string equality after only the whitespace-only check, so a padded sentinel counts as an explicitly supplied value rather than as the mask.
- `MEMORY.md`'s local-key standard — a new key takes only a name and gets a backend-generated value, while an edit keeps the stored value when the field is blank or masked — already describes this behavior, so no `MEMORY.md` or navigation change was required; this note records why the guard exists and where it is enforced.
- Regression coverage lives in `api_fusion::tests::new_keys_with_mask_placeholder_get_a_random_secret` and `api_fusion::tests::save_config_generates_secret_for_new_keys_with_blank_or_masked_value`; both pass.
- No active note is superseded: `notes list` returns only the terminal-independent-provider record (terminal sync), the bilingual-note-triplets record (note format) and [API Fusion Local Key Creation Uses a Name Dialog](../feature/2026-09-17-api-fusion-local-key-name-dialog.md) (local-key create flow); that dialog record likewise covers collecting the name for a new local key, but the two records state different decisions and neither supersedes the other.
