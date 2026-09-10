# Project agent guidance

This file and `AGENTS.md` are the complete shared sub-agent contract. Role files must not override them.

## Required sequence

1. Read `MEMORY.md`, `.ai-workflow/index/navigation.json` and `.ai-workflow/index/navigation.md`.
2. Use `ai-workflow context locate` for known features before any discovery.
3. If the index cannot resolve a feature, request bounded File Explorer discovery with explicit roots.
4. Update `MEMORY.md` and `navigation.json` immediately in the same change whenever architecture, ownership, agent responsibilities, public symbols, paths or workflow rules change, then regenerate and validate `navigation.md`.

Agent results are Markdown under `## Output checklist` using `### Status`, `### Summary`, `### Evidence` and `### Support Requests`; File Explorer uses `### Found Paths`. JSON envelopes are prohibited; v2 manifest JSON is unchanged.

## Role guidance

- Planning clarifies one business-impact issue per turn, gets explicit approval, and freezes `spec.md` and `plan.md`.
- Plan-to-tasks previews and validates the task graph before creating immutable task files.
- Coding uses delegated TDD for one approved task: Todo list, temporary
  project-local worktree, red-green loop, scoped verification and per-step
  commit. Unsplit plans delegate one sub-agent per step, split plans one per
  task in dependency order, and small bugs or requests as one complete unit.
  The orchestrator continues automatically unless blocked.
- Backend and Frontend edit only exact task scopes; Test writes scoped behavior
  tests when delegated, verifies authorized commands and never changes product
  code. Test authoring must precede implementation when a test is required.
- File Explorer is read-only bounded discovery. Researcher performs cited public research and is read-only.
- Documentation Maintainer updates only authorized `MEMORY.md`, navigation and documentation paths; `navigation.json` is authoritative. After checks, it delegates the local commit to Git Operator with exact changed paths and evidence.
- Spec Review and Standards Review are read-only gates. Task Worker delegates work without editing or testing.
- Git Operator alone runs Git, stages exact paths and uses `$git-message`; no remote mutation or unrelated changes.

All roles must stop and report a bounded support request when scope, evidence or frozen inputs are insufficient. Never weaken checks or expand authority silently.
