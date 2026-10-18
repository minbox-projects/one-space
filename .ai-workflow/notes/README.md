# Agent Notes

Agent Notes are the project's proposal and decision records. This README is the single source of their format and governance. MEMORY describes current standards (how); notes explain why. Both must remain consistent in the same change. Notes and the project contract travel with Git; only `.ai-workflow/plans/` is ignored.

## Structure and format

Every note is a three-file sibling pair under `{lifecycle}/{class}/YYYY-MM-DD-topic-title.*`: the English body `YYYY-MM-DD-topic-title.md`, the Chinese body `YYYY-MM-DD-topic-title.zh.md`, and the consistency record `YYYY-MM-DD-topic-title.i18n.yaml`. The date is the topic's first proposal date and the topic is English kebab-case. Lifecycles are `proposed`, `implemented`, `rejected` and `archived`. Each has exactly the supported classes: `architecture`, `bug-fix`, `feature`, `process`, `simplification` and `testing`. Both languages carry equal authority, must say the same thing, and merge as one complete triplet.

Start both bodies with `# Agent Note: <title>`, a blank line, then `Status: <value>`. Active status matches its directory: `proposed`, `implemented`, or `rejected — <one-line reason>`. Archived notes retain `Status: implemented`, immediately followed by `Archived: YYYY-MM-DD`. After that header block, the English body carries `English | [中文](<topic>.zh.md)` and the Chinese body carries `[English](<topic>.md) | 中文`, each on its own line and followed by a blank line.

The body starts with `## Problem`. Required sections contain actual content and occur in this order:

| Lifecycle | Required sections | Constraints |
| --- | --- | --- |
| proposed | Problem, Proposal, Alternatives considered, Acceptance criteria, Risks | Meaningful technical sections may appear between required sections. |
| implemented | Problem, Decision, Alternatives considered, Consequences | Remove unimplemented planning sections, including Proposal, Plan, Migration plan and Acceptance criteria. |
| rejected | Problem, Proposal, Alternatives considered | Preserve the original proposal and useful rejection rationale; include the reason in Status. |
| archived | The implemented structure at sealing | Include the archive date and a matching manifest entry. |

Both bodies keep the same English structural elements: the `# Agent Note:` prefix, section headings, table headers, `Status` and its values, field names, file paths and dates. Only the natural-language prose is translated. The two sides must mirror each other's heading depths and order, verbatim code blocks, table row and column counts, list kinds, ordered-list starts and item counts, and link targets with their query or fragment suffixes. Record only alternatives actually considered and why they were declined.

`<topic>.i18n.yaml` is the pair's consistency record: a header comment plus exactly two lines mapping each side's basename to its git blob hash as of the last confirmed-consistent state. Re-record it only after both sides say the same thing, with `ai-workflow notes pairing --project <root> --write <note>`; `--all` re-records every complete pair as an explicit choice. `ai-workflow notes pairing --list` reports `missing`, `out-of-sync` or `ok` without failing. Do not generate a central INDEX, copied historical records or invented placeholder notes. There are no legacy-format exemptions. A complete empty tree with an empty archive manifest is valid; a missing tree or management file is not.

## Maintenance and lifecycle

A note is required only for a lasting decision rationale that code, tests and existing documentation cannot adequately explain, such as the considered alternatives, ownership, guarantees, data semantics or reintroduction conditions. Behavior changes, file counts and added tests are insufficient on their own to require a note. Local, mechanical, UI, copy and formatting changes without such lasting rationale are exempt.

Planning schedules record maintenance. Major unimplemented work belongs in proposed. The change delivering a decision moves its note to implemented and rewrites the body to describe delivered facts using Decision and Consequences; changing only Status is insufficient. Rejected proposals move to rejected with their reason. Move or rewrite the triplet as a whole and re-record it. Semantic review verifies actual delivery and sufficient, truthful rationale; structural validation alone cannot prove these.

Keep implemented facts such as paths, symbols, defaults and implementation structure current with code in the same change. Rewrite current facts rather than appending a running log. Reuse the existing note that owns the same decision instead of duplicating it; record only a changed recorded decision or its lasting rationale as a new note linked to the prior one.

A note stays self-contained after its plan is gone: a note body describes the delivered result, decision or behavior in durable project terms such as behavior, paths, symbols, guarantees, ownership and validation evidence, and never anchors to transient plan-internal identifiers such as task IDs, step numbers, phase numbers, `REQ`/`AC` IDs or plan directory names. The workspace cross-repository plain plan-ID note reference is the only sanctioned exception.

An existing active note that still carries such an identifier is cleaned by rewriting the anchor into a durable description while preserving its decision, alternatives, consequences and evidence claims; both language sides change together and the pair is re-recorded. Cleanup happens when the note is next maintained or in an authorized bounded cleanup pass. Archived notes are sealed and never cleaned.

For every new note, search related active notes within an authorized scope and assess full, partial or no supersession. Partial supersession retains and cross-links the records. Full consolidation may delete the old active note only after the current owner preserves all unique rationale, alternatives, consequences, verification requirements and known coverage gaps, and repairs inbound links. Alternatively retain history under the archive policy. Removing only part of an implementation must not be described as removing the entire capability.

## Archive policy

Archive based on future guidance value, never age, length or quotas. Keep implemented notes that still explain alternatives, ownership, important guarantees, data semantics or conditions for reintroduction. Archive only a completed implemented decision with little future guidance but historical decision value. Only implemented notes may be archived. Reject obsolete proposals instead.

Retention is a decision, not a default. Keep an implemented record active while its alternatives, ownership, important guarantees, data semantics or reintroduction conditions still guide future work; archive one whose future guidance is spent but whose historical decision value remains; and delete an active implemented record that only describes small UI adjustments or purely mechanical changes without lasting decision value. A small bug fix, performance change, new capability or substantive behavior decision never qualifies for deletion merely because its implementation is small. Retain a rejected record only while it prevents a plausible, meaningful mistake. An authorized deletion removes the complete triplet and repairs inbound links, and existing supersession preservation and the frozen-archive prohibitions below keep applying.

Archiving may only move a complete triplet, insert the archive date in both bodies and repair active inbound links; it must not rewrite the decision. Once sealed, do not modify, move, delete or reorder archived triplets. Historical outbound links are not current link-validity gates and must not be repaired by rewriting sealed history.

`archived/manifest.json` starts as `{ "version": 1, "files": {} }`. Its `files` map registers every archived artifact, keyed by notes-root-relative `archived/<class>/<note>.md`, `.zh.md` or `.i18n.yaml` path with `sha256:<hex>` values; governance files and the manifest itself are excluded.

After the move and semantic review, run `ai-workflow notes archive --project <root> --seal`. It verifies every existing entry's presence and bytes, validates the complete new triplet, then atomically appends its artifact entries. It does not select, move or rewrite notes, and must never replace existing digests to accept changed history. With no new records, sealing makes no change. Mechanical sealing does not replace review of supersession and future guidance value.

## Lookup and validation

Use `ai-workflow notes list --project <root>` for active proposed, implemented and rejected records; only implemented records describe delivered decisions. Add `--archived` explicitly for historical records. Output is deterministic JSON `{ "entries": [...] }` with `path` (the English body), `title`, `lifecycle`, `class`, `date` and `status`; lookup creates no index and reads no retired decision store.

Run read-only `ai-workflow notes validate --project <root>` to check the complete tree, legal paths and dates, title and status, required section content and order, the language switchers, the mirrored structure, the recorded pair hashes, relative Markdown file links between active notes, and archive integrity. Missing counterparts, sidecars or records fail, as do stale recorded hashes. Active links to missing notes fail; archived outbound links are not checked or repaired. Archived artifacts and manifest entries must correspond one-to-one; changed bytes, missing files and unsealed extras fail.

Success returns `{ "valid": true, "errors": [] }` with exit 0; failure returns errors identifying relative paths and reasons with a nonzero exit. Validation does not repair files. Review separately establishes whether content is truthful, sufficient and delivered, and whether supersession preserves its meaning.
