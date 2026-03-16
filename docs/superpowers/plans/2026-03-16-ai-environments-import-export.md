# AI Environments Import Export Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add AI environment import/export so users can export all model environments with decrypted secrets and import them back with a conflict prompt that lets them choose overwrite or create new.

**Architecture:** Add dedicated provider export/import Tauri commands on top of the existing decrypted legacy provider view and encrypted provider persistence layer. Surface the feature in the AI Environments UI with export/import actions and a conflict resolution modal that preflights the import file before applying user choices.

**Tech Stack:** React, TypeScript, Tauri commands, Rust serde/JSON, i18next

---

## Chunk 1: Backend Commands

### Task 1: Define import/export payloads

**Files:**
- Modify: `src-tauri/src/app_store.rs`

- [ ] Add serializable structs for provider export payload, import preview rows, and import resolution requests.
- [ ] Reuse the existing legacy provider view shape for exported provider content so secrets are exported decrypted.
- [ ] Keep active tool bindings in the export format so imports can optionally restore active environments.

### Task 2: Implement export command

**Files:**
- Modify: `src-tauri/src/app_store.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] Add `providers_export` Tauri command that loads providers via the existing decrypted view and writes a pretty JSON file.
- [ ] Include metadata such as version and exported timestamp.
- [ ] Return exported file path and count for frontend success messaging.

### Task 3: Implement import preview and apply commands

**Files:**
- Modify: `src-tauri/src/app_store.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] Add a preview command that reads an import file, validates it, and marks each provider as `new` or `conflict`.
- [ ] Add an apply command that accepts per-provider decisions: `overwrite` keeps the imported provider id, `new` generates a fresh id.
- [ ] Preserve encryption guarantees by saving through `save_providers_state`.
- [ ] Restore imported active bindings only when the resolved provider ids exist after import.

## Chunk 2: Frontend UX

### Task 4: Add import/export actions to AI Environments

**Files:**
- Modify: `src/components/AiEnvironments/index.tsx`

- [ ] Add `Import` and `Export` buttons near the environments list header.
- [ ] Use Tauri dialog `save` for export and `open` for import.
- [ ] Show success and error messages with the existing local message state.

### Task 5: Build conflict resolution modal

**Files:**
- Modify: `src/components/AiEnvironments/index.tsx`

- [ ] After file selection, call the preview command and show imported providers in a modal.
- [ ] For conflict rows, let the user choose `overwrite` or `new`.
- [ ] Add quick actions to apply one choice to all conflicts.
- [ ] Submit the final decisions to the apply command, then reload provider state and keep the user on the current screen.

## Chunk 3: Strings And Verification

### Task 6: Add translation strings

**Files:**
- Modify: `src/i18n.ts`
- Modify: `en_keys.txt`
- Modify: `zh_keys.txt`

- [ ] Add labels, descriptions, confirmation text, and result text for export/import and conflict choices.

### Task 7: Verify behavior

**Files:**
- Modify: `src-tauri/src/app_store.rs` if tests are practical

- [ ] Run targeted frontend or type checks if available.
- [ ] Run targeted Rust checks for new Tauri commands.
- [ ] Sanity-check a sample export/import JSON path and confirm overwrite/new logic compiles.
