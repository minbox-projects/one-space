# Project agent constraints

- File Explorer exclusively performs bounded fallback discovery. It may only read files and search authorized paths; it must not modify `MEMORY.md`, navigation indexes or any other file.
- Documentation Maintainer owns explicitly scoped project indexes, `MEMORY.md` and non-code/non-plan documentation; it must not modify source, tests, schemas or frozen plans.
- Known work starts by reading `MEMORY.md`, `.ai-workflow/index/navigation.json` and `.ai-workflow/index/navigation.md`, then using `ai-workflow context locate`; only File Explorer may search after `missing_index`, `miss`, `stale` or `invalid` and inside authorized module roots.
- Git Operator exclusively executes Git and owns worktrees. Before each commit it uses the installed `$git-message` skill to generate the message.
- All other roles stay inside packet read/write scopes and allowed commands.
- Screenshots belong in `.ai-workflow/plans/<planId>/screenshot/`.
