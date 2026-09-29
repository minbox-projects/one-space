# Agent Note: AI Terminal Session Workflow Presets and Runs Are Removed Full-Stack

Status: implemented

[English](2026-09-29-ai-session-workflow-presets-removal.md) | 中文

## Problem

为精简应用，AI Terminal Sessions 的工作流预设与运行记录能力已被移除。它涵盖预设编辑器、依赖检查与应用流程、运行历史与重放、快速栏预设选择器、OmniSearch 的预设与运行结果、设置中的同步范围、十个 `workflows_*` 命令，以及存储同步、迁移与 provider-id 重映射处理。

## Decision

本次为全栈移除，不保留开关或回退路径。前端界面及其调用点已删除（`src/components/WorkflowPresetsPanel.tsx`、`src/components/RecentWorkflowRuns.tsx`、`src/lib/workflows.ts`），后端 `workflows` 模块及其十个 `workflows_*` 命令也已删除。本地与共享同步、local-data 镜像、启动迁移与 provider-id 重映射不再触碰 `workflow_presets.json` 与 `workflow_runs.json`。`SyncPolicy.workflow_presets` 已从类型、默认值与序列化中移除，而遗留 `config.json` 仍可加载：未知字段被忽略且该键不再被写出。既有的本地与共享工作流数据文件有意保持原样并处于惰性状态。`src-tauri/src/runtime_profiles.rs` 与独立的 AI Workflow Model Switcher 工具箱功能保留。

## Alternatives considered

- 仅移除前端、保留后端命令与存储：未采纳，因为这会留下不可达代码，并为没有入口的功能保留持续的同步、迁移与重映射处理。
- 同时移除工具箱 AI Workflow Model Switcher：未采纳，因为它是独立于 AI Terminal Sessions 范围之外的功能，拥有自己的 profile 存储与命令。
- 删除既有工作流数据文件：未采纳，因为这是破坏性的且没有必要——没有该功能时这些文件处于惰性状态，保留它们还能让旧版本读取自己的数据。

## Consequences

- AI Terminal Sessions 页面、快速栏、OmniSearch 与设置同步范围中不再有任何工作流入口；被移除的 `workflows_*` 命令作为未知命令失败，因此陈旧的前端构建无法调用它们，只会收到错误而不会崩溃。
- 既有 `workflow_presets.json` 与 `workflow_runs.json` 文件及其共享 profile 副本在磁盘上保持字节一致，且不会创建新的工作流共享副本。
- 设置同步范围不再提供已移除的范围；`sync_policy` 仍携带 `workflow_presets` 的遗留 `config.json` 可加载并保留其他所有范围值，下次保存写出的 policy 不含该键。
- `src-tauri/src/runtime_profiles.rs` 中的 `materialize_strict_profile` 与 `cleanup_stale_runtime_profiles` 已无调用方并产生 dead-code 警告，但因会话启动仍对既有会话使用 `runtime_env_for_profile` 而保留。
- 既有 `data/runtime_profiles/` 产物不再由任何工作流路径创建或清理，但 strict 模式会话启动仍通过 `runtime_env_for_profile` 读取并复用既有 profile：该函数要求 `data/runtime_profiles/<id>/` 目录已存在，会补齐其 `home` 与 `xdg_*` 子目录并在其中写入 `.last_used`，因此不得将其视为惰性产物或予以删除。
- 没有任何活动 Note 被全部或部分取代：Notes 树中没有关于已移除工作流预设与运行记录能力的记录，而 [AI Workflow Model Switcher 记录](../feature/2026-09-22-ai-workflow-model-switcher.md) 与 [AI Workflow Profile 保存与激活记录](../feature/2026-09-23-ai-workflow-profile-save-activation.md) 仍然有效，因为该独立功能被保留。
