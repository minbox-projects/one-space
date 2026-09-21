# Agent Note: Bilingual Note Triplets

Status: implemented

English | [中文](2026-09-17-bilingual-note-triplets.zh.md)

## Problem

Agent decision records were effectively single-language, so Chinese-speaking contributors and English-speaking contributors did not share one authoritative record. The upgraded `.ai-workflow/notes/README.md` now mandates an English body plus Chinese body plus `.i18n.yaml` consistency record as equal-authority siblings, but `MEMORY.md` still lacked that standard, leaving the workflow constraint inconsistent with notes governance.

## Decision

Adopted the bilingual triplet for every note: the English body `YYYY-MM-DD-topic-title.md` and the Chinese body `YYYY-MM-DD-topic-title.zh.md` carry equal authority and say the same thing, with only natural-language prose translated and structural elements (`# Agent Note:` prefix, headings, `Status` values, field names, paths, dates) kept in English. Consistency is recorded only with `ai-workflow notes pairing --write` after both sides agree. `MEMORY.md` was aligned in the same change so the current standard matches the README, and this note records the decision rationale.

## Alternatives considered

- Keep single-language notes: declined because it preserves the split readership and contradicts the upgraded README, which has no legacy-format exemptions.
- Machine-translate all history into triplets: declined because the tree is empty so there is no history to migrate, and bulk translation would create unverified records instead of one reviewed triplet for the current decision.

## Consequences

- Every non-mechanical change maintains a complete triplet in the same change; `MEMORY.md` records the current standard while notes record why.
- `ai-workflow notes validate` and `ai-workflow notes pairing --list` gate consistency: missing counterparts or stale hashes fail validation.
- The starting tree was empty, so no migration or supersession was needed; the `notes list` supersession check returned no related active notes.
