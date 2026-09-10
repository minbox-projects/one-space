# Project agent constraints

These instructions are authoritative for every sub-agent. Installed role files contain only host metadata and identity and must not override this file or `CLAUDE.md`.

## Shared context and maintenance

- Before repository work, read `MEMORY.md`, `.ai-workflow/index/navigation.json` and `.ai-workflow/index/navigation.md`.
- For indexed known features, use `ai-workflow context locate --project <absolute-project-root> --feature <id> --verify`; do not search first.
- A missing or empty index is normal for a new project. `missing_index` and a
  feature `miss` do not by themselves block implementation when the frozen
  plan supplies an explicit boundary; request bounded File Explorer discovery
  only when that boundary is unclear. For an unsplit plan, do not invoke
  File Explorer merely because the feature is absent from the index. `stale` or `invalid` still require
  bounded discovery or index repair before relying on indexed paths.
- Navigation JSON is authoritative. Updating `MEMORY.md` and `.ai-workflow/index/navigation.json` is mandatory and immediate whenever architecture, ownership, agent responsibilities, public symbols, paths or workflow rules change. Regenerate and validate `navigation.md` in the same change.
- Agent results are Markdown under `## Output checklist` with `### Status`, `### Summary`, `### Evidence` and `### Support Requests`; File Explorer uses `### Found Paths`. JSON envelopes are prohibited; v2 manifest JSON is unchanged.

## Workflow roles

- Planning asks one business-impact question at a time, obtains approval, and creates frozen `spec.md` and `plan.md`.
- Plan-to-tasks validates the frozen pair, previews the complete graph, obtains approval, and creates immutable `tasks/<taskId>.md` files. It never edits frozen plans.
- Coding implements either one approved task or an approved frozen plan with
  TDD: Todo list, one project-local temporary worktree, failing behavior test,
  minimal implementation, scoped checks, per-step commit and cleanup. It never
  creates workflow manifests or run records. Coding work must be delegated:
  unsplit plans delegate one sub-agent per step, split plans delegate one
  sub-agent per task in dependency order, and small bugs or requests delegate
  as one complete unit. The orchestrator continues automatically while no
  blocker, failed gate, missing authorization or user decision exists.
- Task Worker coordinates one task and delegates implementation, testing and Git work; it does not edit files, search broadly, run tests or run Git.
- After Coding implementation completes, exactly one Spec Review and one Standards Review must run before any worktree merge. Findings go to the user for repair selection; unresolved review findings block merge.

## Agent permissions

- Backend and Frontend edit only exact task write scopes. Frontend screenshots stay under `.ai-workflow/plans/<planId>/screenshot/`.
- Test writes or updates behavior tests only within an explicit test scope when
  delegated, runs only explicitly allowed commands, changes no product code,
  and reports exit status, evidence, skipped checks and failures truthfully.
- File Explorer is read-only and may search only authorized roots. It never edits files or guesses paths.
- Researcher handles every technology, project, concept, product, topic or keyword research request using public sources and citations. It never edits files.
- Documentation Maintainer owns only explicitly scoped `MEMORY.md`, navigation indexes and non-code documentation. JSON navigation is authoritative and Markdown is generated from it. After completing checks, it must call Git Operator with exact changed paths and evidence for the local commit; it must not commit directly.
- Spec Review checks requirements, acceptance criteria, testability, scope and coverage. Standards Review checks changes against `MEMORY.md`. Both are read-only.
- Git Operator is the only role allowed to run Git, stages only explicit paths, invokes `$git-message` before commits, preserves unrelated changes and performs no remote mutation.
- All agents stop on missing scope, contradictory frozen inputs, infrastructure failure or out-of-scope requests and return a bounded support request. Never weaken tests or silently expand authority.
