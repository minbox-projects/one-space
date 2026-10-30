# Agent Note: Cloud Agent Plan Execution

Status: implemented

English | [中文](2026-10-30-cloud-agent-plan-execution.zh.md)

## Problem

Running a saved frozen plan on the self-hosted cloud host previously required either an interactive terminal session or an Issue-driven run. An Issue run starts from Issue text rather than a local plan, and an interactive session depends on a person staying attached to answer prompts. Users needed to choose a saved workspace and a never-started frozen plan from the desktop, have the cloud execute it against the latest fetched remote code, keep the job alive after the desktop disconnects, and receive a reviewable pull request without automatic merge. The design also had to prevent a repeated submission from starting a second coding session, avoid uploading local source modifications, and define exactly which permissions and questions the cloud may answer on the user's behalf.

## Decision

Plan execution is cloud-owned. The desktop submits a plan and then reads durable job state from the cloud instead of persisting a second execution authority. Each job stores its admission record, request metadata and an immutable `upload/` copy under `~/.config/onespace-cloud-agent/plan-tasks/<job-id>/`, written atomically under a host-local file lock.

Admission keys on canonical repository identity plus the frozen plan ID, not on a local display name or root path. Identity is the GitHub or GitHub Enterprise host and the provider-resolved owner/repository, normalized across SSH and HTTPS spellings, host case and a `.git` suffix, then confirmed through `gh repo view`. The same identity with the same base branch and the same uploaded bytes returns the existing job in every state, including completed, failed and cancelled; a new client request ID becomes an alias, and a changed base branch or payload is refused rather than replacing the accepted upload. The admitted bytes stay separate from the execution copy, so coding writing its own plan record into the clone cannot change the replay comparison.

Each job gets a fresh owned clone under the configured cloud workspace root. Preparation fetches the selected branch immediately before coding, captures the full fetched SHA as the run's base and creates the job delivery branch at that SHA. A shared checkout is never reused by repository basename, and unrelated work is never reset or cleaned. Root mapping is part of invocation rather than of the frozen files: the execution context declares the original local project root and its equivalent cloud root, and task scopes and command paths prefixed by the original root are mapped to the clone when they are assembled, while the frozen bytes and digests remain unchanged. A reference outside that mapping or a machine-only prerequisite is a visible blocker, not a reason to broaden scope or rewrite a frozen document.

Submitting delegates single-use tool approval for the job. Owned permission requests are answered with `once`, native `deny` remains in effect, and the runner never sets a global `always` permission or edits host permission settings. Question automation is syntactic: a question is answered automatically only when every question is single-choice and has exactly one option label ending in `(Recommended)`, `(推荐)` or `（推荐）` after normalizing surrounding whitespace and English case, and the original label is returned. The rule never infers a recommendation from option order, a sole option or description text. It applies to every matching question, including ones that mention scope or acceptance, so it is deliberately not a semantic filter and does not promise that a marked choice is safe; the workflow still owns scope and acceptance enforcement. Free-text, multiple-choice, missing-marker and ambiguous-marker questions remain `waiting_input` for same-session takeover through `opencode attach`.

Completion is workflow evidence rather than model prose. Publication requires an observed successful `coding` skill tool invocation, the matching coding-owned `implementation.yaml` marked `completed`, the full final SHA it records equal to the delivery branch HEAD, and a clean delivery worktree. The runner does not independently infer test or review verdicts from the transcript. A verified tree identical to the captured base ends as `no_changes` without an empty commit or pull request.

A verified change is published with a normal non-force push of the job delivery branch and one pull request created with an explicit repository, base and head. A matching pull request is reconciled before creation and after an uncertain creation response, so a lost response does not create a duplicate. Publication returns the pull request URL and final SHA, preserves commits and reports its stage on failure, and never merges the remote pull request: merging remains a human action.

Cancellation stops only the job's owned processes and prevents a publication that has not started, while retained state and clones remain readable and already-issued effects are reported truthfully. Failed or cancelled work is not rerun automatically, and a new execution requires a newly frozen plan identifier. A missing installed prerequisite or an unsupported OpenCode interface stops before editing, and an older cloud installation reports that an upgrade is needed instead of guessing a fallback.

None of these prompt or workflow constraints is an operating-system sandbox. The service keeps its existing execution privileges and `--yolo` model; containment comes from prompts, scope checks and owned processes, while host hardening remains an operator responsibility.

## Alternatives considered

- Identify jobs by local workspace path, display name or a new content fingerprint: declined because a local path is not stable across machines and the canonical repository plus plan ID, base branch and uploaded bytes already identify one execution, so another fingerprint would add no decision value.
- Reuse a shared checkout keyed by repository basename: declined because unrelated repositories can share a basename and reuse would mix unrelated work or require destructive resets.
- Persist cloud execution state on the desktop: declined because a disconnected or switched client would become a stale authority; the cloud remains the single state owner.
- Classify recommended options semantically or allow-list questions about scope and acceptance: declined because that would turn the runner into a business judge and make behavior unauditable; the syntactic rule is predictable, logged and leaves acceptance enforcement to the workflow.
- Merge the pull request automatically after a verified result: declined because delivery was explicitly a human-reviewed pull request.
- Rerun failed or cancelled jobs automatically: declined because side effects may already have been issued and ownership is uncertain after an interruption; manual handling or a newly frozen plan is required.
- Rewrite frozen plan paths to cloud roots: declined because rewriting would change the frozen bytes and digests; mapping at invocation preserves the plan's authority.
- Run plan execution inside an operating-system sandbox: declined for this capability because it would change the retained privileges and `--yolo` prerequisites; the absence of a sandbox is documented as a limitation rather than implied by prompt text.

## Consequences

- Job state, admitted uploads, request records and clones remain under `~/.config/onespace-cloud-agent/plan-tasks/<job-id>/` and the configured workspace root; closing the desktop or SSH does not stop an accepted job, and reconnecting reads the cloud authority.
- Repeated submissions reuse the original job and its PR or state; a changed base branch or payload is rejected, and an old-client retry alias cannot start a second coding session.
- A question that does not match the syntactic recommendation rule blocks progress until a person attaches to the exact session and answers it; operators should not expect the runner to judge whether a marked option is safe.
- Publication can stay blocked when the completion evidence is absent, even if the session looks idle; conversely, `no_changes` creates no commit or PR.
- The cloud host must already provide the `ai-workflow` CLI, the `coding` skill, native roles, Python, `tmux`, Git, authenticated `gh` and a supported OpenCode v1 interface baselined at 1.19.5; this capability installs none of them.
- Recovery is manual and non-destructive: cancel or stop owned jobs before reverting service code, keep uploads, state, clones and published PRs for inspection, and do not delete remote branches or close PRs as a rollback side effect.
- Related active notes: [cloud agent hardening](../architecture/2026-10-29-cloud-agent-hardening.md) remains current for the service-wide gateway, Issue, session and deployment guarantees; this record adds the plan-execution rationale without superseding it.
- Verification is the server unit and integration suites, including the end-to-end flow over a local bare Git repository with simulated protocol and GitHub responses, plus the no-provider native OpenCode smoke; real Linux deployment with a live model remains separate deployment evidence, and the absence of an OS sandbox is not mitigated by these checks.
