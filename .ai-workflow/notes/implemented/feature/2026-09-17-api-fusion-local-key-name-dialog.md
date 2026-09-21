# Agent Note: API Fusion Local Key Creation Uses a Name Dialog

Status: implemented

English | [中文](2026-09-17-api-fusion-local-key-name-dialog.zh.md)

## Problem

The `Api Keys` panel used to render the create form inside its header: an inline text input for the new key's name sat next to the `Add key` button, the button stayed disabled until that input held text, the Enter key submitted it, and the input's `Label` / `标签` wording described a list attribute rather than the name being requested. Because the name state lived in `LocalKeyList.tsx`, opening the create flow, validating it and clearing it after a submit were part of rendering the key list, and the panel header permanently reserved space for a control that mattered only while creating a key.

## Decision

Creating a local key now happens in a dialog. `LocalKeyList.tsx` keeps only the `Add key` button, which is enabled whenever the panel is not busy and calls a new `onAdd` prop; its `onSave` prop and `labelInput` state are gone. `src/components/ApiFusion/index.tsx` owns the new `isKeyDialogOpen` state, opens it from `onAdd`, and renders `LocalKeyDialog.tsx` (`src/components/ApiFusion/LocalKeyDialog.tsx`) with `open`, `onOpenChange`, `busy` and `onSave`. The dialog holds the name in its own state, resets it on every open, keeps `Save` disabled until the trimmed name is non-empty, and submits `{ id: "", label: name.trim(), value: "", enabled: true, created_at: 0 }`, so the create contract is unchanged: only a name is collected and the backend still generates the secret. Its title reuses `apiFusionAddKey`, its description is the new `apiFusionKeyDialogDesc` key, its field label reuses `apiFusionKeyLabel`, and its placeholder is `apiFusionKeyLabelPlaceholder`.

## Alternatives considered

- Keep the inline header input and only rewrite its copy: declined because the create form would stay in the list panel header, and the name state, its validation and its reset would remain in `LocalKeyList.tsx`, keeping a create interaction coupled to list rendering.
- Collect the key value in the dialog as well: declined because the create flow's established contract is that a new local key takes only a name and receives a backend-generated secret ([New Local API Keys Never Persist the Mask Placeholder](../bug-fix/2026-09-17-api-key-mask-sentinel-never-persisted.md)), so a value input would be ignored.
- Keep the `Label` / `标签` wording for the field: declined because the field asks for the key's name and the create flow needs nothing else, so the new `Name` / `名称` label and the `Key name` / `密钥名称` placeholder describe the field accurately.

## Consequences

- The `Api Keys` header no longer contains a name input, and `Add key` is enabled whenever the panel is not busy; a click only opens the dialog, so the previous disabled-until-named button, the Enter-to-submit path and the inline clear-after-submit behavior no longer exist.
- `LocalKeyDialog.tsx` owns the name state, clears it on every open, disables `Save` until the trimmed name is non-empty, and disables both of its buttons while the panel is busy; closing the dialog without saving changes nothing.
- `LocalKeyList`'s contract changed from `onSave` to `onAdd`, and `api-fusion`'s entry `src/components/ApiFusion/index.tsx` owns `isKeyDialogOpen` and forwards the dialog's `onSave` to the existing `handleSaveKey`, which still calls `api_fusion_upsert_key`.
- The submitted payload is unchanged (`id: ""`, trimmed label, `value: ""`, `enabled: true`, `created_at: 0`), so the backend keeps returning a generated `sk-fusion-` secret.
- `src/i18n.ts` now carries `apiFusionKeyLabel` = `Name` / `名称` (was `Label` / `标签`), `apiFusionKeyLabelPlaceholder` = `Key name` / `密钥名称` (was `Label` / `标签`), and the new `apiFusionKeyDialogDesc` for both languages.
- Behavior coverage in `src/components/ApiFusion/ApiFusion.test.tsx` asserts that the header has no name input, that `Add key` is enabled without a name, that clicking it opens the dialog, that the dialog has no key-value field, that `Save` is disabled until a name is entered, and that the exact `api_fusion_upsert_key` payload is sent.
- `LocalKeyDialog.tsx` is registered in the `api-fusion` feature's `related_files` and `read_scope`, and `navigation.md` is regenerated from `navigation.json` with `ai-workflow context validate` passing; because `context validate` rejects an indexed path that does not exist at the validated root, a file that exists only in a worktree cannot be referenced from the single-source index.
- No decision is superseded: the backend mask guard and the terminal-sync contract are unaffected. The create-flow sentence in [New Local API Keys Never Persist the Mask Placeholder](../bug-fix/2026-09-17-api-key-mask-sentinel-never-persisted.md) now also attributes the name collection to `src/components/ApiFusion/LocalKeyDialog.tsx`, so the two records overlap on the create flow and state the same ownership.
- `MEMORY.md`'s local-key standard (a new key takes only a name and the value is backend-generated) still matches this behavior, so it is unchanged.
