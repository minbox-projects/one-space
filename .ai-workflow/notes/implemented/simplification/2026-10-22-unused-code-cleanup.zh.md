# Agent Note: Unused Code Audit Cleanup Retires Dormant MCP Runtime and Interfaces

Status: implemented

[English](2026-10-22-unused-code-cleanup.md) | 中文

## Problem

项目带有已审计但无生产消费者的代码：休眠的 MCP 会话运行时与模板匹配器、已注册的 IPC 封装及其专属实现链、不可达的备份与模型池 UI 动作及被忽略的 props、孤儿模块、类型、CSS、依赖、资源与提示文件，以及无人读取的双语键。尽管没有任何可达工作流依赖它们，这些表面仍持续消耗构建时间、维护与审查注意力。较早的项目记录还把部分已退役代码描述为有意保留，导致记录的标准与代码树不一致。

## Decision

本次审计移除了已证实的孤儿代码，而不是用开关隐藏它们或留下休眠的兼容表面。

- 退役休眠 MCP 会话运行时（`mcp_runtime`）与 `mcp_templates::find_mcp_template_for_server` / `server_matches_template` 匹配器及其专属测试。保留 MCP 模板目录（`get_mcp_templates`）及其 list/get 命令（`list_mcp_templates`、`get_mcp_template`）、托管服务器、消息与终端投影。
- 撤回已审计的未用 IPC 名称及其专属 helper，包括备份创建 IPC、版本探测查询、代理 get/save、同步与迁移状态 helper、Claude profile 解析、调试解密、直接 SSH 列表封装、未用的 Skills 与 Subagents 配置、导出、本地扫描/导入、更新检查、rescan 与仓库刷新命令、Skills 更新 diff/apply 组合，以及 Webhook 停止封装。移除命令是有意的接口撤回：旧调用方现在收到标准的未知命令错误。
- 保留可达工作流：Subagents 更新 preview/apply 与两个文件夹导入、内部恢复前备份创建、SSH 快照加常用端口 upsert/delete、MCP 模板目录/list/get 与托管服务器和消息，以及原生退出。
- 将 Webhook 监听器停止统一到原生退出：`shutdown_runtime_services` 幂等调用 `WebhookServerManager::global().stop()`，不再暴露停止 IPC。
- 把 `BackupManager` 变为全量备份管理器，列出每个备份并保留恢复、删除与保留策略清理。移除手工创建、被忽略的 active-tool prop 与缺少工具警告；恢复前安全备份仍保留在后端内部。
- 移除被忽略的模型池卡片/列表删除回调链，同时保留模型池编辑器中的删除，并在不破坏注册表加载的前提下移除模型切换器未使用的可见性字段。
- 移除已审计的孤儿 UI 模块、类型、CSS 规则、npm 与 Cargo 依赖、资源与根提示/脚本文件，以及已审计的 i18n 键。保留动态族（例如运行时组合的 workspace skill rule 键）与共享活跃键。
- 持久化格式、迁移与加密行为均未改变；已存储用户数据、遗留读取器与既有 schema 版本保持完整。

## Alternatives considered

- 为可能的未来用途保留休眠 MCP 运行时、匹配器与未用命令接口：未采纳，因为没有生产消费者，且休眠的公共接口仍要承担兼容、审查与维护，却不服务任何工作流。
- 按名称删除，包括看起来未使用的回调：未采纳，因为 Subagents preview/apply 工作流、文件夹导入、恢复前备份、快照/常用端口变更与模型池编辑器删除都可达，会被破坏。
- 删除共享或动态 i18n 前缀下的每个键：未采纳，因为运行时组合键与共享键都是活跃的，仅凭字面搜索无法证明未使用。
- 清理时广泛升级依赖：未采纳，因为本次清理只需移除不可达的直接依赖；无关版本变动会在审计范围之外引入风险。
- 把删除 Subagents 更新工作流或恢复前备份作为清理的一部分：未采纳，因为两者都是活跃行为，而备份是恢复的安全网。

## Consequences

- 每个保留的工作流仍可用，而移除的表面不再增加构建、测试或审查成本。
- 已退役 IPC 名称的旧调用方收到标准未知命令错误，而不是兼容 shim；这是有意的接口变更，回退清理提交即可恢复代码与接口。
- 验证证据：限定 Rust 套件通过（app_runtime 44、mcp_templates 1、mcp_servers 17、skills 47、subagents 17、ssh_tunnels 68、protocol_router 16、backup 5、webhook_catcher 3、app_store 135），前端门禁 10 个文件 362 个测试通过，`npm run build` 退出 0，i18n 每种语言从 3,534 个叶键降到 3,368 个，英文/中文集合一致，键清单各 1,038 项。依赖清理移除四个 npm 包与两个直接 Rust crate，未升级保留包。
- 已知边界：`npm run lint` 仍退出 1，未改动的 `src/components/AiGateway/UsageLogsPanel.test.tsx` 中有一个既有的 `react-hooks/globals` 错误，另有既有警告；本次清理没有引入该错误。
- 部分取代：[AI Workspace Removed End to End](2026-09-29-ai-workspace-removal.md) 仍是工作区移除的权威记录，本记录只修复并交叉链接其原先关于保留运行时的表述；两条记录均保持活跃。未做持久化格式、迁移或加密变更。
- 重新引入条件：未来需要该休眠 MCP 客户端或匹配器的功能必须显式重新加入，并带有生产消费者与行为测试，而不是恢复已删除模块。
