# Agent Note: AI Workflow Profile Save and Activation Are Separate Actions

Status: implemented

[English](2026-09-23-ai-workflow-profile-save-activation.md) | 中文

## Problem

AI Workflow 模型切换器最初将保存 profile 与激活 profile 合并，因此用户若只想持久化对既有 profile 的编辑，也可能同时改变当前活跃 profile。界面新建的 profile 还需要在首次保存后提供明确选择，而不能在 profile 矩阵尚未持久化时询问。

## Decision

保存与激活是彼此独立的操作。Save 持久化所选矩阵但不改变活跃 profile；Activate 通过既有的 `ai_workflow_activate_profile` 流程操作该 profile 已保存的 YAML。既有 profile 的 Save 不会弹出提示。对于在界面中创建的 profile，首次成功 Save 后会出现 Yes/No 提示：No 保持 profile 已保存但未激活；Yes 则单独激活现已持久化的 YAML。Save 失败时绝不提示。如果单独激活失败，已保存的矩阵仍保持持久化。

界面行为实现在 `src/components/AiWorkflowModelSwitcher/AiWorkflowModelSwitcher.tsx`；持久化使用新的 `ai_workflow_save_profile` 命令，激活使用既有的 `ai_workflow_activate_profile` 命令。既有的组合命令 `ai_workflow_save_and_activate_profile` 及其快照回滚行为仍为兼容性保留。

## Alternatives considered

- 保持 Save 与 Activate 合并：未采纳，因为保存对既有 profile 的编辑可能意外切换活跃 profile。
- 先保存，再询问是否激活：采纳，因为用户明确选择激活前已完成持久化，选择 No 仍会保留已保存但未激活的 profile。
- 保存前先询问：未采纳，因为这可能询问用户是否激活尚未持久化的矩阵。

## Consequences

- 既有 profile 的 Save 仅执行持久化操作，不改变活跃 profile 状态；Activate 仍是针对已保存 YAML 的显式操作。
- 界面新建的 profile 仅在首次成功保存后提示一次。Save 失败不会触发激活提示或激活尝试。
- 激活发生在持久化之后。激活失败不会丢弃已成功保存的矩阵；组合后端命令及其回滚仍保留，但分离后的界面操作不使用它。
- 更广泛的切换器实现及其原有备选方案仍记录在 [AI Workflow Model Switcher Delivers 9-by-3 Matrix with Backend Profile Commands](2026-09-22-ai-workflow-model-switcher.md) 中。
