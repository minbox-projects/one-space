# Agent Note: AI Workspace Removed End to End

Status: implemented

[English](2026-09-29-ai-workspace-removal.md) | 中文

## Problem

OneSpace 在原生终端 AI Sessions 之外还带有一个应用内 AI 对话工作区：`AiWorkspace` 与 `SmartWorkspaceHub` 表面、模型中心、助手连接设置、`Schedules` 页面、快捷与划词助手浮窗，以及 `aiWorkspace`/`aiAssistant`/`assistantToolCalls`/`assistantMcpDisplay` 封装。其后端是 `src-tauri/src/ai_assistant` 模块（commands、conversations、model request、providers、scheduler、schedules、settings、state、tools、types、tests）与 `assistant_mcp` 桥接，暴露 `ai_workspace_bootstrap`、`workspace_settings_*`、`workspace_model_roles_*`、`workspace_assistant_*`、`workspace_conversation_*`、`workspace_automation_*`、`workspace_schedule_*`、`workspace_quick_assistant_*` 与 `workspace_selection_assistant_*` 命令族、`provider_connection_test`、`provider_models_fetch`、`assistant_mcp::*` 命令与助手窗口命令。该功能与 AI Sessions 终端流程重复，自带按 profile 存放的 `ai_workspace_state.json` 与 `data/mcp/assistant_mcp_tool_previews.json`；其能力面在 [Toolbox Plugin Registry Replaces Hand-Maintained Tool Lists](../architecture/2026-09-25-toolbox-plugin-registry.md) 变更撤出 `notes_search` 时已经收紧。

## Decision

整个工作区被删除，而不是隐藏、加特性开关或只保留后端。

- 前端表面与封装全部删除，连同其托盘、侧边栏、Launcher、OmniSearch、设置、应用内 Documentation 与 i18n 入口。Quick AI Session Bar 与所有 `quick-ai` 入口保留。
- 后端 `ai_assistant` 树与 `assistant_mcp.rs` 删除，invoke 处理器不再注册工作区命令、provider 测试与抓取命令、`assistant_mcp::*` 命令、调度器初始化与助手窗口命令。命令表面属于公共接口，因此这是有意的公共接口变更：旧前端构建收到标准的未知命令错误而不是崩溃。
- 启动执行一次性 best-effort 清理。`app_runtime::run_app::run()` 在 `config::seed_dev_gateway_files_on_start()` 之后调用 `cleanup_removed_ai_workspace_files()`；包装函数经 `crate::get_data_dir()` 解析当前 profile 的本地数据目录并委托 `cleanup_removed_ai_workspace_files_in(base)`，后者恰好对 `ai_workspace_state.json` 与 `data/mcp/assistant_mcp_tool_previews.json` 调用 `fs::remove_file`。文件不存在时忽略，其他失败记录并在下次启动重试，绝不阻塞启动，绝不触碰任何其他路径。
- 共享状态保留：MCP 服务器列表（含播种的 `mcp-exa` 与 `mcp-context7` 服务器）、消息中心、MCP Servers 页面以及保留的 `mcp_runtime` 与 `mcp_templates` 模块都不在清理范围内。

## Alternatives considered

- 保留助手后端、只删除 UI：未采纳，因为命令、调度器、provider 与存储会变成有维护成本但不可达的无人消费表面，之后的每次构建、测试与审查都要继续承担它。
- 仅隐藏入口或把功能置于特性开关之后：未采纳，因为隐藏不会删除任何代码、命令、调度器或按 profile 的数据文件，公共命令表面继续存在，留下仍欠维护与兼容承诺的休眠功能。
- 清理助手预览缓存时一并删除播种的 MCP 服务器（`mcp-exa`/`mcp-context7`）：未采纳，因为 MCP 服务器列表与保留的 MCP Servers 页面及终端投影共享，删除用户可见的共享数据会改变被移除功能之外的行为。
- 把两个功能自有文件留在磁盘上而不在启动时清理：未采纳，因为已无任何代码读取它们，它们会作为死掉的按 profile 状态永久留存；已批准的代价是被清理的文件无法恢复。

## Consequences

- 不再有任何工作区、助手、调度器或助手浮窗：侧边栏、托盘、Launcher、OmniSearch、设置与应用内文档都没有入口，导航索引也没有对应 feature。文档（`docs/USAGE.md`、README）与路线图文档在同一变更中更新，`MEMORY.md` 记录清理约定。
- 仍调用已删除命令的旧前端构建收到标准的未知命令错误而不是崩溃；由于被清理的文件无法恢复，该构建的助手工作区从空状态开始。
- 清理幂等且有界：恰好删除当前 profile 本地数据目录下的两个固定文件，无关的 `data/mcp` 文件保留（由新的 Rust 测试断言），共享 MCP 列表与消息中心不受影响。
- 助手桥接删除后，`mcp_runtime` 与 `mcp_templates::find_mcp_template_for_server` 在代码树中不再有生产调用方；两者均原样保留，且 `mcp_templates` 仍通过其 `list_mcp_templates` 与 `get_mcp_template` 命令服务 MCP Servers 页面。调度器曾是 `chrono-tz` 依赖的唯一依赖方，该依赖因此从 `Cargo.toml`/`Cargo.lock` 移除。
- 助手能力契约不再存在：`AgentToolPolicy`、能力快照、模型工具定义、默认 agents、会话提示、分派器与全部助手 i18n 键都已删除，而 `quick_ai_shortcut` 与 subagents 使用的 Bot 图标保留。
- 部分取代：[Toolbox Plugin Registry Replaces Hand-Maintained Tool Lists](../architecture/2026-09-25-toolbox-plugin-registry.md) 仍是注册表决策及其其余移除项的权威记录；本记录只取代其中关于仍然存在的助手能力面（能力徽标与开关）的表述。两条记录均保留并互相交叉链接。
