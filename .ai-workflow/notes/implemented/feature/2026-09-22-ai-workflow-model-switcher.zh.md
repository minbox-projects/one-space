# Agent Note: AI Workflow Model Switcher Delivers 9-by-3 Matrix with Backend Profile Commands

Status: implemented

[English](2026-09-22-ai-workflow-model-switcher.md) | 中文

## Problem

切换和配置 AI Workflow subagent profiles 此前只能通过手动修改 `~/.config/ai-workflow/profiles/` 下的 YAML 文件，或在无法总览各宿主模型分配的情况下执行 `ai-workflow profile activate <name>` CLI 命令。同时管理 Codex、Claude 与 OpenCode 多宿主的开发者无法在一个统一界面中检视角色与宿主间的模型映射，缺乏针对各宿主本地配置的候选补全，也没有安全的批量调整机制。直接修改 profile YAML 还会绕过 schema 校验，并在托管 subagent 文件发生漂移时面临激活失败或配置损坏的风险。

## Decision

OneSpace 交付独立的工具箱工具 `ai-workflow-model-switcher`，由专用的后端 Tauri 命令和 9×3 角色/宿主编辑矩阵承载。

该实现分为前端工具箱矩阵界面与安全后端运行时两部分：

1. 前端工具箱矩阵界面：构建于 `src/components/AiWorkflowModelSwitcher/`，并在六个关键点完成布线（`src/lib/navigation.ts`、`src/lib/moreToolPresentation.ts`、`src/lib/launcherToolVisibility.ts`、`src/components/MoreToolsHub.tsx`、`src/components/Launcher.tsx` 与 `src/App.tsx`）。提供带有活跃标记的 profile 选择器、映射 9 个规范 subagent 角色（`backend`、`documentation-maintainer`、`file-explorer`、`frontend`、`git-operator`、`researcher`、`spec-review`、`standards-review`、`test`）与 3 个宿主（`codex`、`claude`、`opencode`）的 9×3 网格、带手动输入降级的模型源下拉框、推理强度选择器（限制为六个枚举值 `low`、`medium`、`high`、`xhigh`、`max`、`ultra`）、按列/按行/全矩阵批量应用、未保存脏状态跟踪、使用 `ai_workflow_save_profile` 且不激活既有 profile 的 Save 操作，以及针对已保存 YAML 调用 `activateProfile` 的独立 Activate 操作。通过此界面新建的 profile 在首次成功 Save 后仅显示一次 Yes/No 确认：No 保持 profile 已保存但不激活；Yes 单独激活已持久化的 profile。Save 失败时不提示，激活失败不丢弃已保存矩阵，保存成功会更新 dirty baseline。
2. 后端 Profile 运行时：在 `src-tauri/src/ai_workflow_profiles.rs` 与 `src-tauri/src/ai_workflow_profiles/` 中实现，注册八个 Tauri 命令（`ai_workflow_list_profiles`、`ai_workflow_get_profile_matrix`、`ai_workflow_get_model_sources`、`ai_workflow_activate_profile`、`ai_workflow_save_profile`、`ai_workflow_save_and_activate_profile`、`ai_workflow_create_profile`、`ai_workflow_delete_profile`）。后端从本地配置文件（`opencode.json` 服务商模型、`config.toml` 顶层及 profile 模型、`settings.json` env 键）聚合宿主候选模型，隔离单源故障以保证其他列正常可用，执行严格 schema 校验（`version: 1.0.0`、成对 model 与 effort 取值、安全 profile 名称），并原子保存 YAML。save-only 命令 `ai_workflow_save_profile` 不调用 CLI，也不修改 active profile。保存并激活流程在写盘前创建目标 YAML 字节快照，解析外部 `ai-workflow` 二进制触发激活，并在激活失败时自动恢复快照。

## Alternatives considered

- 纯 CLI 备选方案（在 `ai-workflow` CLI 中直接增加交互式 `profile edit` 或矩阵配置引导）：未采纳，因为 OneSpace 是开发者统一管理 AI 环境、网关和终端会话的桌面工作台；在终端中通过多轮交互编辑跨越 27 个单元格并支持多种批量操作体验受限，而在桌面工具箱中提供图形化矩阵能直观对比跨宿主配置。
- 前端直接读写 `~/.config/ai-workflow/profiles/` 下的 YAML 文件：未采纳，因为 OneSpace 保持前端展示与文件系统修改的严格边界；由 Tauri 后端统一负责 YAML 校验、原子写入、CLI 执行与快照回滚，能够保证安全性并防止产生不一致的脏文件。
- 联网实时通过 `/v1/models` 接口抓取远端模型列表：未采纳，因为切换器基于各本地工具配置（`opencode.json`、`config.toml`、`settings.json`）中已配置且受支持的模型运作；依赖网络接口会引入网络延迟、请求失败以及密钥暴露风险，而读取本地配置保证了离线可用性。

## Consequences

- 后端作为 profile YAML 变更与激活命令的唯一权威来源；前端仅通过 `src/lib/aiWorkflowProfiles.ts` 中的类型化 Tauri 命令交互，绝不直接访问配置路径。仅保存与激活彼此分离，保存既有 profile 不会隐式改变当前 active profile。
- 模型源优雅隔离故障：某个工具（如 Codex 或 OpenCode）的配置文件不可读仅将对应列标记为可操作错误并降级到手动输入，其他宿主列的模型选项仍可完全正常使用。
- save-only YAML 写入为原子操作，不运行 CLI 或改变 active profile。保存并激活流程通过快照回滚确保激活失败（如 CLI 缺失或托管 agent 文件被篡改产生冲突）时，目标 profile 文件立即恢复为之前的原始字节，完整保留 CLI 错误信息且不留存脏 YAML。
- API 密钥与敏感凭证绝不被读取、记录或序列化到 profile YAML 中。
- 导航索引在 `.ai-workflow/index/navigation.json` 中登记 `ai-workflow-model-switcher` feature（归属模块根 `frontend`，owner 为 `frontend`），重新生成了 `navigation.md`，并在 `MEMORY.md` 中记录了矩阵编辑与回滚规范。
- 将既有 profile 的 Save 设为仅持久化操作、并仅在界面新建 profile 首次成功保存后询问的原因记录在 [AI Workflow Profile Save and Activation Are Separate Actions](2026-09-23-ai-workflow-profile-save-activation.md)；本记录保留更广泛的切换器决策与备选方案。

## Decision

OneSpace 交付独立的工具箱工具 `ai-workflow-model-switcher`，由专用的后端 Tauri 命令和 9×3 角色/宿主编辑矩阵承载。

该实现分为前端工具箱矩阵界面与安全后端运行时两部分：

1. 前端工具箱矩阵界面：构建于 `src/components/AiWorkflowModelSwitcher/`，并在六个关键点完成布线（`src/lib/navigation.ts`、`src/lib/moreToolPresentation.ts`、`src/lib/launcherToolVisibility.ts`、`src/components/MoreToolsHub.tsx`、`src/components/Launcher.tsx` 与 `src/App.tsx`）。提供带有活跃标记的 profile 选择器、映射 9 个规范 subagent 角色（`backend`、`documentation-maintainer`、`file-explorer`、`frontend`、`git-operator`、`researcher`、`spec-review`、`standards-review`、`test`）与 3 个宿主（`codex`、`claude`、`opencode`）的 9×3 网格、带手动输入降级的模型源下拉框、推理强度选择器（限制为六个枚举值 `low`、`medium`、`high`、`xhigh`、`max`、`ultra`）、按列/按行/全矩阵批量应用、未保存脏状态跟踪、直接激活以及带详细报告的保存并激活操作。
2. 后端 Profile 运行时：在 `src-tauri/src/ai_workflow_profiles.rs` 与 `src-tauri/src/ai_workflow_profiles/` 中实现，注册五个 Tauri 命令（`ai_workflow_list_profiles`、`ai_workflow_get_profile_matrix`、`ai_workflow_get_model_sources`、`ai_workflow_activate_profile`、`ai_workflow_save_and_activate_profile`）。后端从本地配置文件（`opencode.json` 服务商模型、`config.toml` 顶层及 profile 模型、`settings.json` env 键）聚合宿主候选模型，隔离单源故障以保证其他列正常可用，执行严格 schema 校验（`version: 1.0.0`、成对 model 与 effort 取值、安全 profile 名称），在写盘前创建字节级快照备份，解析外部 `ai-workflow` 二进制触发激活，并在激活失败时自动回滚 YAML 以防止配置处于脏状态。

## Alternatives considered

- 纯 CLI 备选方案（在 `ai-workflow` CLI 中直接增加交互式 `profile edit` 或矩阵配置引导）：未采纳，因为 OneSpace 是开发者统一管理 AI 环境、网关和终端会话的桌面工作台；在终端中通过多轮交互编辑跨越 27 个单元格并支持多种批量操作体验受限，而在桌面工具箱中提供图形化矩阵能直观对比跨宿主配置。
- 前端直接读写 `~/.config/ai-workflow/profiles/` 下的 YAML 文件：未采纳，因为 OneSpace 保持前端展示与文件系统修改的严格边界；由 Tauri 后端统一负责 YAML 校验、原子写入、CLI 执行与快照回滚，能够保证安全性并防止产生不一致的脏文件。
- 联网实时通过 `/v1/models` 接口抓取远端模型列表：未采纳，因为切换器基于各本地工具配置（`opencode.json`、`config.toml`、`settings.json`）中已配置且受支持的模型运作；依赖网络接口会引入网络延迟、请求失败以及密钥暴露风险，而读取本地配置保证了离线可用性。

## Consequences

- 后端作为 profile YAML 变更与激活命令的唯一权威来源；前端仅通过 `src/lib/aiWorkflowProfiles.ts` 中的类型化 Tauri 命令交互，绝不直接访问配置路径。
- 模型源优雅隔离故障：某个工具（如 Codex 或 OpenCode）的配置文件不可读仅将对应列标记为可操作错误并降级到手动输入，其他宿主列的模型选项仍可完全正常使用。
- 原子保存与快照回滚机制确保当激活失败（如 CLI 缺失或托管 agent 文件被篡改产生冲突）时，目标 profile 文件立即恢复为之前的原始字节，完整保留 CLI 错误信息且不留存未保存的脏 YAML。
- API 密钥与敏感凭证绝不被读取、记录或序列化到 profile YAML 中。
- 导航索引在 `.ai-workflow/index/navigation.json` 中登记 `ai-workflow-model-switcher` feature（归属模块根 `frontend`，owner 为 `frontend`），重新生成了 `navigation.md`，并在 `MEMORY.md` 中记录了矩阵编辑与回滚规范。
