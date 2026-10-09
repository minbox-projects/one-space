<!-- ai-workflow:section contract-introduction:begin -->
# ai-workflow project contract

This contract applies to the entire project and every participating agent, including sub-agents. Read it explicitly before repository work; do not rely on a host recursively loading hidden directories or following Markdown links. Installed role files must not override this contract.

<!-- ai-workflow:section contract-introduction:end -->
<!-- ai-workflow:section shared-context:begin -->
## Shared context and maintenance

- Before repository work, read `MEMORY.md`, `.ai-workflow/index/navigation.json` and `.ai-workflow/index/navigation.md`.
- For indexed known features, use `ai-workflow context locate --project <absolute-project-root> --feature <id> --verify`; do not search first.
- A missing or empty index is normal for a new project. `missing_index` and a feature `miss` do not block implementation when the frozen plan supplies an explicit boundary. Request bounded File Explorer discovery only when that boundary is unclear; do not invoke it merely because an unsplit plan's feature is absent. `stale` or `invalid` require bounded discovery or index repair before relying on indexed paths.
- Navigation JSON is authoritative. Update `MEMORY.md` and `.ai-workflow/index/navigation.json` immediately when architecture, ownership, agent responsibilities, public symbols, paths or workflow rules change. Regenerate and validate `navigation.md` in the same change.
- Fixed task context includes this contract, MEMORY and both navigation files. Add relevant notes and governance files to explicit bounded read/write scopes; do not require every task to read the entire history.
- Agent results are Markdown under `## Output checklist` with `### Status`, `### Summary`, `### Evidence` and `### Support Requests`; File Explorer uses `### Found Paths`. JSON envelopes are prohibited; v2 manifest JSON is unchanged.

<!-- ai-workflow:section shared-context:end -->
<!-- ai-workflow:section agent-notes:begin -->
## Agent Notes

Read `.ai-workflow/notes/AGENTS.md` and `.ai-workflow/notes/README.md` before maintaining notes. The README is the single source for format, lifecycle, supersession and archive governance. Planning schedules the relevant note work; the change that lands the decision owns its record and lifecycle transition. MEMORY records current standards (how), while notes record why; keep them consistent in the same change.

<!-- ai-workflow:section agent-notes:end -->
<!-- ai-workflow:section change-routing:begin -->
## Change routing

Classify every request before starting and state the class in one line.

- Direct change — a small requirement, feature adjustment or defect fix with clear, bounded intent and one observable outcome: implement it directly without Planning, without `spec.md`, `plan.md` or task files, and without the dual-axis review. Run the relevant checks and add one focused regression test for a defect.
- Planned change — a new feature, unclear or contested requirements, more than one materially different design, or a change to a public interface, persistent format, cross-module or cross-stack behavior, migration, compatibility or this contract: run Planning first, implement the frozen plan, and keep the dual-axis review gate.
- Mechanical change — a typo, copy, comment, formatting or test-only adjustment with no observable behavior change: implement it directly and run only the narrowest relevant check.

Never run Planning to restate a request with clear, bounded intent, and never label a change direct to skip required checks or evidence. Ask the user only when these rules cannot classify the request.

<!-- ai-workflow:section change-routing:end -->
## Workflow roles

- Planning asks one business-impact question at a time, obtains approval, and creates frozen `spec.md` and `plan.md` for a planned change only.
- Plan-to-tasks validates the frozen pair, previews the complete graph and its parallel phases with the critical path, obtains approval, and creates immutable `tasks/<taskId>.md` files plus `tasks/execution-order.yaml`, the frozen schedule: an ordered list of non-empty `parallel` phases covering every task exactly once, with every dependency in a strictly earlier phase and no overlapping normalized write-scope paths inside one phase (directory containment included). The schedule is machine-readable, has no `.zh.md` or `.i18n.yaml` sibling, and matches the approved preview. It never edits frozen plans.
- Coding creates one project-local temporary worktree under `<project>/.worktrees/<name>` before implementation and performs all implementation, validation and per-step commits inside that single worktree; planning, TDD and review depth remain proportional to risk. Git Operator materializes the project's entire gitignored state into the worktree, excluding the `.worktrees/` container, so `.ai-workflow/plans/` stays visible and single-source at the project root while MEMORY, navigation and notes arrive with the worktree through Git. For a split plan, `tasks/execution-order.yaml` is the only schedule: phases run in file order, every task of the current phase is dispatched concurrently inside that single worktree with test work before implementation in each task, the whole phase is waited for and verified, and each task's write scope is committed serially through Git Operator one commit at a time before the next phase starts; a missing or invalid order stops the run before execution, and coding never recomputes phases from `depends_on` or falls back to serial execution. Coding creates no workflow manifest or run record other than the single implementation record at `<project>/.ai-workflow/plans/<planId>/implementation.yaml`, which holds `plan_id` with `status: in-progress` and an ISO 8601 `started_at` in the UTC+08:00 timezone before the first implementation step and becomes `status: completed` with an ISO 8601 `completed_at` in the UTC+08:00 timezone and the final commit SHA after the final merge and owned cleanup; it is the only permitted run record.
- Backend and Frontend edit only exact task write scopes; Frontend screenshots stay under `.ai-workflow/plans/<planId>/screenshot/`.
- Test writes or updates behavior tests only within an explicit delegated test scope, runs only explicitly allowed commands, changes no product code, and reports exit status, evidence, skipped checks and failures truthfully.
- File Explorer is read-only and may search only authorized roots. It never edits files or guesses paths.
- Researcher handles every technology, project, concept, product, topic or keyword research request using public sources and citations. It never edits files.
- Documentation Maintainer owns explicitly scoped MEMORY, navigation indexes, notes and non-code documentation, returns exact changed paths and validation evidence to the primary orchestrator, and must not invoke Git itself.
- Spec Review checks requirements, acceptance criteria, testability, scope, coverage and actual delivery. Standards Review checks consistency with MEMORY and its referenced notes rules. Both are read-only.
- Git Operator is the only role allowed to run Git, stages only explicit paths, invokes `$git-message` before commits, preserves unrelated changes and performs no remote mutation.

<!-- ai-workflow:section orchestration:begin -->
## Orchestration

The primary orchestrator directly dispatches Git Operator and every specialist in dependency order; no coordinator role exists. For split and unsplit coding it directly dispatches Git Operator, File Explorer, the implementation role, Test, both reviews simultaneously in one parallel batch (never Spec first and Standards after Spec completes), an optional repair, and finalization. After the Documentation Maintainer returns exact changed paths and validation evidence, the primary orchestrator directly dispatches Git Operator for the local commit. All agents stop on missing scope, contradictory frozen inputs, infrastructure failure or out-of-scope requests and return a bounded support request. Never weaken tests or silently expand authority.
<!-- ai-workflow:section orchestration:end -->
<!-- ai-workflow:section workspaces:begin -->
## Workspaces

A workspace is a root repository that composes child repositories as local git submodules; `.gitmodules` is the source of the `submodule boundary` and keeps each child isolated, so a child's work never leaks into the root or a sibling. Each participating repository keeps its working tree at its declared workspace-root-relative path, and only its slice and plan artifacts live under the repository path convention `<root>/.ai-workflow/plans/<planId>`; the workspace plan records the `repository-level order` in which slices are delivered.

The workspace commands are `ai-workflow workspace distribute --plan <directory>` to hand each child its slice and `ai-workflow workspace status --plan <directory>` to report each repository's completion and delivery commit. `workspace distribute` verifies each participating repository through `read-only Git` before writing; `workspace status` validates the workspace manifest before reporting `valid: true`, derives its `order` and `next_repository` from the frozen `tasks/execution-order.yaml` repository first-appearance including the pending `workspace` root tasks, reports `workspace_root_entry` with `record`, `tasks_delivered` and a nullable `delivery_commit`, requires every child `record` `completed` with its slice `present`, a matching `plan_id` and a full-SHA `delivery_commit`, plus the root `tasks_delivered` for `ready_for_finalization`, stays filesystem-only and runs no Git. Delivery-commit verification reads each source repository through `read-only Git` before any index mutation; these reads of the workspace working tree are an explicit `exception` to the `worktree confinement` that otherwise keeps every write inside the coding worktree, and `moving submodule checkouts after the pin is out of scope`.

Cross-repository references stay `plain plan-ID text`. The `workspace root` owns the `decision` note, while `each repository` owns its own `delivered facts`; split the two, and each side updates only its own notes and `MEMORY.md`. Planning keeps the reserved `workspace` root a dependency-free `prefix` delivered separately from `finalization`, requires cross-repository acceptance criteria to name their owning repository with exact bounded `read-only` source checks, and never checks out a child automatically. The root's own tasks are delivered as an optional `root_tasks_commit` in the single root `implementation.yaml`, which stays `status: in-progress` until finalization so the workflow still has one record with only `in-progress` and `completed`. During `finalization`, the already-running correct root session executes directly without reopening itself, the root Git Operator verifies each delivery commit with a read-only `git cat-file -e <sha>^{commit}` existence check before any index mutation, and each verified delivery commit is pinned into the workspace as an authorized `pointer`, staging only those pointer paths and authorized workspace-root files.

A workspace plan that declares at least one non-root participating repository must be split by plan-to-tasks before implementation, and completing that split runs `ai-workflow workspace distribute --plan <directory>` to hand each participating repository its slice. A workspace plan whose `workspace_repos` declares only the reserved workspace root entry may be implemented unsplit.

<!-- ai-workflow:section workspaces:end -->
<!-- ai-workflow:section work-boundary-synchronization:begin -->
## Work-boundary synchronization

Adopted projects synchronize their managed workflow instructions at work boundaries. The installed host entry invokes `ai-workflow sync-hook --host <current-host>`, which reads the host's native JSON payload on stdin; do not run that form by hand without the payload. At a cooperative phase boundary use `ai-workflow sync-hook --host <current-host> --phase --project <actual-root>`. That `--phase` form on every host, and the OpenCode stdin gate, print the raw gate JSON: a top-level `decision` of `allow`, `deny` or `skip`, `project`, `context`, optional `authority`, and an optional nested `report` carrying `status`, `verified` and `proceed`. The Claude Code and Codex stdin gates instead emit host-native output: `systemMessage`, and `hookSpecificOutput` with `hookEventName`/`additionalContext` plus `permissionDecision` and `permissionDecisionReason` on a `PreToolUse` deny; a `UserPromptSubmit` block carries a top-level `decision` of `block` with `reason`. These native gates never expose the raw `allow`/`deny`/`skip` decision. Every handled form exits 0 even on a deny, so the host follows its protocol rather than the exit code. Follow a raw `decision`: `deny`, or a `conflict` or `failed` `report.status`, blocks the phase; a warning `report.status` proceeds with visible context and makes no freshness claim; `skip` means no adoption was found. The manual `ai-workflow sync [project]` command prints a `SyncReport` with `status`, `verified` and `proceed` and no `decision`.

This preflight is a narrow instruction-maintenance exception to coding-worktree confinement: it writes only the managed workflow instruction files of the actual current project root or the active coding worktree, and it performs no Git, no staging and no product edit. It discloses every created or updated dirty path, and a clean-baseline or task-scope collision produces a bounded support request instead of silent staging, worktree rewriting or broader authority.

After a safe patch changes the contract or owned workflow rules, the updated `.ai-workflow/AGENTS.md` must be explicitly read or injected before ordinary work continues. Frozen `spec.md`, `plan.md`, task files and their declared repository/write scopes are never synchronized, and a contradiction between new authority and a frozen scope stops that phase with an explanation.

<!-- ai-workflow:section work-boundary-synchronization:end -->
