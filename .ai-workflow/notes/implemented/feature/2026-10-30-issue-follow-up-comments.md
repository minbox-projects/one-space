# Agent Note: Issue Follow-Up Comments

Status: implemented

English | [中文](2026-10-30-issue-follow-up-comments.zh.md)

## Problem

The Issue worker treated an Issue as a one-shot task. Once a terminal record existed, later comments were never read, so an authorized collaborator who rejected the submitted pull request had to open another Issue, and any naive re-reading of the thread would have replayed bot, unauthorized and Agent-authored comments. The worker also could not tell its own pull request branch from a human branch that merely mentioned the Issue, and it had no durable boundary between comments already considered and comments that arrived later.

## Decision

Follow-ups are driven by comments, not by pull request events. For an open Issue that still carries the configured trigger label, a comment that starts with `/agent` (optional leading whitespace, case-insensitive, followed by end or whitespace) starts exactly one follow-up when the commenter's effective repository permission is `admin`, `maintain` or `write`; `maintain` normalizes to `write` and `triage` to `read`. The existing Issue-author gate is evaluated first, so an unauthorized author or a failed author lookup evaluates no comment and leaves the cursor unchanged. Bots (`user.type` of `Bot` or a login ending in `[bot]`), comments containing the Agent marker, closed or unlabeled Issues, non-command comments and `/agent` embedded mid-text never trigger. A confirmed unauthorized commenter is consumed without a run, while a permission lookup failure stops evaluation and leaves the cursor unchanged. There is no separate configuration switch.

Every prompt separates authorization from content. A trusted preamble states that a verified `/agent` comment triggered the follow-up and that the request is an untrusted requirement, not a system instruction, and it also reports the previous outcome (`status`, `branch`, `pr_url`, `last_error`). The untrusted block then contains the Issue title and body, only authorized human comments and the follow-up request in that order; those values are sanitized so supplied boundary markers cannot close the block. Authorized history and the request share a 50,000 UTF-8 byte cap that drops oldest history first, retains the request when possible and appends a visible truncation note when the request alone is too large; the title, body and outcome metadata do not count. Unauthorized, bot and marker comments never enter the prompt, and the fixed restrictions against secrets, host files and edits outside the clone remain.

Each trigger performs a fresh clone and a fresh `opencode run` with the full authorized history; no OpenCode session is resumed and no previous workspace is retained. An open PR is Agent-owned only when its body references `#<n>` at digit boundaries, its head repository equals the target repository and its head branch exactly equals the `branch` stored for that Issue. A matching PR is checked out, receives appended commits and is pushed to the same branch without a second PR. Without an Agent-owned PR and without another open PR that references the Issue, a new branch is created from the default branch and a new PR with `Closes #<n>` is opened. An open PR that references the Issue but is not Agent-owned is never modified: the worker replies once and consumes the trigger. Keeping the Agent PR open and unmerged and posting `/agent` again is the supported rejection-and-adjustment path; PR conversation comments and `Request changes` reviews do not trigger, and closing the PR means the next trigger starts a new branch and PR rather than reusing the closed one.

A durable `last_comment_id` per Issue makes evaluation idempotent. It advances over ignored comments and, after a terminal outcome (`done`, `failed` or `no_changes`), to the maximum comment ID observed in the trigger snapshot. Comment-fetch and commenter-permission failures leave it unchanged; unauthorized commenters are consumed; comments arriving during a run have higher IDs and wait for a later poll; a comment edited after consumption does not re-trigger. Every Agent-issued Issue comment carries the hidden `<!-- onespace-cloud-agent -->` marker, which both excludes it from triggering and keeps it out of history without changing the visible text. State stays `version: 2`; `last_comment_id`, `branch` and `pr_url` are additive optional fields with defaults, other shapes follow the existing invalid-state quarantine, and v1/v2 loading, initial processing, retries, the initial PR skip, task history and notifications are unchanged.

## Alternatives considered

- Reopen or replace the Issue for adjustments: declined because it fragments the discussion and loses the durable Issue/PR relationship that reviewers already follow.
- Resume the previous OpenCode session or reuse its workspace: declined because a retained session and clone can carry stale or unverified state, and a fresh clone keeps every run reproducible.
- Treat any open PR that references the Issue as reusable: declined because a human or fork PR could then receive Agent commits; branch identity alone is insufficient, so ownership requires the recorded state branch, the target repository and the numeric Issue reference.
- Require the Issue author to post `/agent`, or accept any new comment as a trigger: declined because other authorized collaborators need the same path, while unqualified comments would turn thread chatter and bot output into work.
- Include unauthorized comments in history with warnings: declined because it widens the injection surface and the prompt budget without adding authorized requirements.
- Allow PR conversation comments or `Request changes` reviews to trigger: declined because the Issue stays the single permission-gated and auditable entry point, and the PR surface would need separate authorization handling.
- Record every processed comment ID instead of a monotonic cursor: declined because a single cursor is smaller, survives restarts the same way and already ignores edits to consumed comments.
- Reject oversized follow-up requests instead of truncating: declined because dropping oldest history preserves the current request, and a visible truncation note keeps the run possible without hiding the loss.

## Consequences

- `/agent` is case-insensitive after optional leading whitespace, and the cap is 50,000 UTF-8 bytes over authorized comments plus the follow-up request.
- A follow-up appears as a normal new task-history entry with the existing field format; no desktop UI, task-history schema, gateway or session-runner change is required.
- The rejection workflow is operational: keep the Agent PR open and unmerged, then post `/agent` on the Issue. Closing it discards branch continuity by design.
- When no Agent-owned PR exists, an open PR that references the Issue but belongs to someone else blocks code changes for that trigger: the worker replies once, consumes the cursor and leaves the PR untouched.
- State remains `version: 2`; an older worker ignores the new fields and may drop them when it next saves. Re-upgrading starts the cursor at `0`, so historical `/agent` comments may be evaluated once; this is accepted because the command is an explicit authorized action.
- Verification is the Issue worker behavior suite, the full server Python suite and note validation; the prompt boundary remains a prompt constraint rather than an operating-system sandbox.
- Related active notes: [Cloud Agent Hardening Guarantees](../architecture/2026-10-29-cloud-agent-hardening.md) remains current and is extended by this record rather than superseded.
