# Agent Note: macOS Tray AI WorkFlow Submenu Shows Installed Agent Models

Status: implemented

[English](2026-10-09-ai-workflow-tray-models.md) | 中文

## Problem

macOS 托盘菜单暴露了页面、服务与快捷键，却没有展示 AI Workflow 模型切换器所管理的 subagent 模型配置，用户必须打开应用才能看到每个角色将使用哪个模型与推理强度。已保存的 active profile YAML 不是该视图的可靠来源：Save 与 Activate 是分离动作，因此持久化的 profile 定义可能不同于各 host 实际已安装的配置，托盘不得把已保存定义当作当前已安装状态呈现。

## Decision

只读后端查询与托盘子菜单展示当前已安装的 agent 配置，而不是已保存的 profile 定义。

- `src-tauri/src/ai_workflow_profiles.rs` 新增公共函数 `get_active_models(home_override: Option<&Path>) -> Result<Option<ProfileMatrix>, String>`，并在 `src-tauri/src/app_runtime/run_app.rs` 注册 Tauri 命令 `ai_workflow_get_active_models`。它读取 `~/.config/ai-workflow/config.yaml` 的 `active_profile`（缺失或空白返回 `None`），再读取三 host 安装的固定九个角色文件：Codex `.codex/agents/<role>.toml` 的 `model` 与 `model_reasoning_effort` 键，Claude `.claude/agents/<role>.md` 的 `model` 与 `effort` YAML frontmatter，以及 OpenCode `.config/opencode/agents/<role>.md` 的 `model` 与 `reasoningEffort` frontmatter。它绝不读取已保存 profile YAML，也绝不运行 CLI。它不做任何变更；缺失的 host 文件使该可选 host 为 `None`，存在但字段缺失或空白的文件产生空串，已存在但格式错误的 config 或 host 文件返回错误而不是陈旧数据。它复用既有的 `ProfileMatrix`、`AgentMatrixRow` 与 `ModelEffort` 类型。
- `src/lib/aiWorkflowProfiles.ts` 新增公共 `getActiveModels` 封装并导出 `AI_WORKFLOW_PROFILE_UPDATED_EVENT = "ai-workflow-profile-updated"`；`activateProfile` 仅在激活成功后派发该 window 事件。
- `src/lib/trayMenu.ts` 为 `TrayMenuState` 增加可选 `aiWorkflow: { profile: ProfileMatrix | null; unavailable?: boolean }`，并把顶层 AI WorkFlow 条目放在 `ai-usage` 之后、`more-pages` 之前。它构建禁用态 profile 头，随后九个角色子菜单，每个含三个禁用 host 行并显示完整 model 与 reasoning effort；当快照缺失、为空或失败时回退到双语的加载、未激活方案、不可用与未设置文案。
- `src/App.tsx` 在挂载时、`main-window-visibility-changed` 事件时、`AI_WORKFLOW_PROFILE_UPDATED_EVENT` 时以及一个仅在主窗口隐藏时贡献的 60 秒定时器上刷新快照，并在清理时移除定时器与监听器。后发起的请求获胜，拉取失败会清空此前的单元格并把子菜单标记为不可用，相同快照避免多余的原生重建，原生菜单应用仍经既有 apply 链串行化。

## Alternatives considered

- 读取 `~/.config/ai-workflow/profiles/<active>.yaml` 下已保存的 active profile YAML 供托盘使用：未采纳，因为 Save 与 Activate 分离，该已保存定义可能不同于已安装配置，托盘会误报当前状态。
- 在 profile 激活时持久化一份快照并让托盘读取该快照：未采纳，因为激活已经写入已安装文件，这些文件就是直接来源，而独立快照会为一个只读展示增加写入路径与陈旧性。

## Consequences

- 托盘中呈现的是新建 agent 将使用的 model 与 reasoning effort。它不声称运行中的进行中会话会热重载，也不读取当前 token 用量。
- 该查询严格只读：缺失或空白 `active_profile` 显示未激活方案消息，已存在但损坏的 config 或已安装文件显示不可用消息而不是陈旧单元格。
- `tray-menu` feature 现在依赖 `ai-workflow-model-switcher` 提供前端封装与类型，`ai-workflow-backend` 把新增命令注册为其第九个核心命令；`MEMORY.md` 与导航索引在同一变更中记录两者。
- 本记录不取代 [Tray menu ownership and contract](../architecture/2026-09-22-tray-menu-ownership-and-contract.md)：左键切换窗口、右键打开原生菜单，且 [Save and Activate separation](2026-09-23-ai-workflow-profile-save-activation.md) 仍是依据已安装文件而非已保存 YAML 的原因。
