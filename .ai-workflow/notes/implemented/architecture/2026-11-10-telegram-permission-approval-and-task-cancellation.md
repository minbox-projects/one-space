# Agent Note: Telegram Permission Approval and Task Cancellation

Status: implemented

English | [中文](2026-11-10-telegram-permission-approval-and-task-cancellation.zh.md)

## Problem

Telegram could start planning and coding through `/planning` and `/coding`, but permission requests raised by the OpenCode child had no remote approval path: an unanswered request could block the run while the operator was away, and approving everything by default would silently authorize destructive shell commands. `/cancel` only cleared bridge bookkeeping, so the local child could keep running and a late result could still be reported as success. Hosted sessions had a related gap: a failed answer injection lost the question, and completion detection could report success while the agent was still waiting for an answer.

## Decision

`src-tauri/src/remote_bot/permission.rs` exposes the pure classifier `classify_permission(permission_type, command)`, returning `PermissionRisk::Routine` or `PermissionRisk::HighRisk` without reading settings or performing I/O. Only an explicit allowlist is routine: read-only inspection, test/build/lint/format tooling, safe local Git including commits without `--amend`, `mkdir`/`touch` and `cd`/`sleep` for shell requests, and the non-shell permission types `read`, `list`, `glob`, `grep`, `edit`, `write`, `patch` and `apply_patch`, which are auto-approved without a card when auto-approval is enabled. Compound segments, substitution, unlisted redirection, wrappers, unknown commands or permission types and empty input are high risk, so ambiguous input fails closed. Any leading `KEY=VALUE` environment assignment before a shell command is also high risk and fails closed, because assignment keys can change command resolution, execution hooks or interpreter and build behavior, so ordinary assignments are no longer treated as transparent wrappers.

`opencode_bridge.rs` reads `auto_approve_permissions` from the persisted config on every request. When enabled, routine requests are answered `once` with no card; when enabled and high risk, and for every request while disabled, the bridge sends a Telegram approval card and tracks the request in memory. A failed card delivery never auto-approves and the request stays pending until timeout. High-risk cards offer only Allow once and Deny; routine cards add Always allow. The first decision replies `once`, `always` or `deny`, edits the card and makes later taps no-ops; Always is forwarded to OpenCode, OneSpace stores no second authorization, and high-risk Always callbacks are ignored. `PERMISSION_TIMEOUT` is 30 minutes: the poller sweeps due requests and replies `deny` once, and only a successful deny commits the timeout by editing the card and notifying the chat; a failed deny reply keeps the request pending with its original deadline so a later sweep retries it, without editing the card or notifying the chat. Resolved, cancelled or task-ended requests never time out and late taps are ignored.

Bridge tasks run in an owned process group on Unix (`process_group(0)`). `/cancel` sends SIGTERM and escalates to SIGKILL within the separate two-second `CANCEL_GRACE`, reporting `Cancelled`, `ForcedCancelled` or `NothingToCancel` truthfully. The read loops observe the cancel flag, a task generation guard suppresses late success or failure cards, pending permission and question cards are edited to cancelled, and task state and pending permissions are cleared. External and hosted sessions and the shared `opencode serve` are untouched. Manager stop, reload, disable, restart or save and app shutdown call bridge `shutdown`, which also terminates the bridge-owned `opencode serve`; `shutdown_runtime_services` invokes `remote_bot::shutdown_runtime()`.

Hosted questions are injected first and cleared only on success, so failed or duplicate answers stay retryable, while in-memory questions whose database call id disappeared are cleared. The idle counter resets and completion is skipped while a question is pending in memory or in the database. `HostedSessionInfo.active_question_summary` is shown in `/host` and `/sessions <hosted session>`, with a `host_question:<session_id>` "View pending question" action that re-sends the current card or reports that the question expired.

## Alternatives considered

- Blanket approval of every permission. Declined because high-risk commands would be silently authorized whenever the poller saw a request, removing the operator from the decision that matters.
- Per-chat OneSpace allowlists or a second authorization store. Declined because it would duplicate OpenCode's own session memory and create two authorities that can disagree about whether an action is still approved.
- A configurable timeout or per-request expiry UI. Declined because the fixed 30-minute budget is what keeps an unanswered request from blocking the agent indefinitely; exposing it invites an effectively unbounded wait.
- A memory-only cancel flag without terminating the child. Declined because the orphan process keeps writing files and consuming resources while the chat is told the task stopped.
- Killing the task but leaving the bridge-owned server running. Declined because stop, reload and shutdown must leave no process owned by the bridge, and a surviving server keeps the old project directory reachable.
- Removing a hosted question before injecting the answer. Declined because a failed injection then destroyed the only record of what the agent was waiting for, with no retry path.
- Marking a hosted session complete while a question is pending. Declined because it reports success while the agent is still blocked and hides that an answer is required.

## Consequences

- Non-shell `read`, `list`, `glob`, `grep`, `edit`, `write`, `patch` and `apply_patch` requests are routine and are auto-approved when `auto_approve_permissions` is on; ambiguous shell input fails closed as high risk, and the allowlist can only grow through an explicit classifier change and a matching test.
- An unanswered request is denied once after 30 minutes and both the card and the chat show the timeout; it never stays pending indefinitely.
- Routine cards can grant Always, but the high-risk path never offers it and ignores a stale or forged high-risk Always callback.
- Pending permissions, questions, task generations and cancellation state live only in memory; `auto_approve_permissions` remains the only setting, no schema or migration changes, and the Telegram callbacks are additive.
- `/cancel` terminates only the bridge-owned task group; external and hosted sessions and the shared `opencode serve` keep running, and the outcome wording distinguishes a clean cancel, a forced kill and nothing to cancel.
- Manager lifecycle operations and app shutdown terminate the bridge-owned task and server, and an already-idle bridge shuts down idempotently.
- Hosted questions survive a failed injection and block completion until answered or detected stale, and pending questions stay visible from `/host` and the session detail with a working re-send action.
- Behavior is covered by the `permission_*`, `cancel_*` and `hosted_*` tests in `src-tauri/src/remote_bot/tests.rs` plus the focused `RemoteBotSettingsPanel.test.tsx` settings-panel test; settings copy states the on/off behavior, the 30-minute timeout, the high-risk no-Always rule and that saving or restarting terminates the current remote task.
