# 09 - 固定提交双轴最终审查

## Goal

在冻结的 fixed point 与 review commit 之间执行且仅执行一次 Standards 与 Spec 双轴最终审查，分别保留发现并先报告用户，不自动修复或扩大到未提交差异。

## Dependencies

08 - 集成验收与稳定评审提交

## Status

ready-for-agent

## Acceptance Criteria

- [ ] 固定并记录可解析的完整 `FIXED_POINT` 与 `REVIEW_COMMIT` SHA，且前者是后者祖先、三点 diff 非空。
- [ ] 审查开始前工作树干净；审查范围只来自冻结端点间的 committed diff 和 commit list。
- [ ] Standards 与 Spec 审查收到完全相同的两个 SHA、diff 命令和 commit list，并分别收到各自要求的标准或已批准计划。
- [ ] 两轴审查各执行一次且上下文隔离，发现分别保留，不合并成单一严重级别。
- [ ] 所有发现先报告用户；未获用户明确授权时不启动修复、复审、提交或整合。

## Verification

```bash
git rev-parse "$FIXED_POINT"
git rev-parse "$REVIEW_COMMIT"
git merge-base --is-ancestor "$FIXED_POINT" "$REVIEW_COMMIT"
test -z "$(git status --porcelain)"
test -n "$(git diff --name-only "$FIXED_POINT...$REVIEW_COMMIT")"
git diff "$FIXED_POINT...$REVIEW_COMMIT"
git log "$FIXED_POINT..$REVIEW_COMMIT" --oneline
```

命令通过后，按 `~/.config/ai-work-flow/routing.md` 委派 Code Reviewer，并检查 Standards 与 Spec 两份独立结果均已返回和报告。
