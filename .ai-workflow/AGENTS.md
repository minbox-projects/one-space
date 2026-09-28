# ai-workflow project contract

This contract applies to the entire project and every participating agent, including sub-agents. Read it explicitly before repository work; do not rely on a host recursively loading hidden directories or following Markdown links. Installed role files must not override this contract.

## Shared context and maintenance

- Before repository work, read `MEMORY.md`, `.ai-workflow/index/navigation.json` and `.ai-workflow/index/navigation.md`.
- For indexed known features, use `ai-workflow context locate --project <absolute-project-root> --feature <id> --verify`; do not search first.
- A missing or empty index is normal for a new project. `missing_index` and a feature `miss` do not block implementation when the frozen plan supplies an explicit boundary. Request bounded File Explorer discovery only when that boundary is unclear; do not invoke it merely because an unsplit plan's feature is absent. `stale` or `invalid` require bounded discovery or index repair before relying on indexed paths.
- Navigation JSON is authoritative. Update `MEMORY.md` and `.ai-workflow/index/navigation.json` immediately when architecture, ownership, agent responsibilities, public symbols, paths or workflow rules change. Regenerate and validate `navigation.md` in the same change.
- Fixed task context includes this contract, MEMORY and both navigation files. Add relevant notes and governance files to explicit bounded read/write scopes; do not require every task to read the entire history.
- Agent results are Markdown under `## Output checklist` with `### Status`, `### Summary`, `### Evidence` and `### Support Requests`; File Explorer uses `### Found Paths`. JSON envelopes are prohibited; v2 manifest JSON is unchanged.

## Agent Notes

Read `.ai-workflow/notes/AGENTS.md` and `.ai-workflow/notes/README.md` before maintaining notes. The README is the single source for format, lifecycle, supersession and archive governance. Planning schedules the relevant note work; the change that lands the decision owns its record and lifecycle transition. MEMORY records current standards (how), while notes record why; keep them consistent in the same change.

## Change routing

Classify every request before starting and state the class in one line.

- Direct change — a small requirement, feature adjustment or defect fix with clear, bounded intent and one observable outcome: implement it directly without Planning, without `spec.md`, `plan.md` or task files, and without the dual-axis review. Run the relevant checks and add one focused regression test for a defect.
- Planned change — a new feature, unclear or contested requirements, more than one materially different design, or a change to a public interface, persistent format, cross-module or cross-stack behavior, migration, compatibility or this contract: run Planning first, implement the frozen plan, and keep the dual-axis review gate.
- Mechanical change — a typo, copy, comment, formatting or test-only adjustment with no observable behavior change: implement it directly and run only the narrowest relevant check.

Never run Planning to restate a request with clear, bounded intent, and never label a change direct to skip required checks or evidence. Ask the user only when these rules cannot classify the request.

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

## Orchestration

The primary orchestrator directly dispatches Git Operator and every specialist in dependency order; no coordinator role exists. For split and unsplit coding it directly dispatches Git Operator, File Explorer, the implementation role, Test, both reviews simultaneously in one parallel batch (never Spec first and Standards after Spec completes), an optional repair, and finalization. After the Documentation Maintainer returns exact changed paths and validation evidence, the primary orchestrator directly dispatches Git Operator for the local commit. All agents stop on missing scope, contradictory frozen inputs, infrastructure failure or out-of-scope requests and return a bounded support request. Never weaken tests or silently expand authority.
