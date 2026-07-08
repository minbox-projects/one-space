---
name: project-code-navigation
description: 当在 OneSpace 中处理 AI 环境、AI 会话、工作空间、MCP、Skills、Subagents、SSH、工作流、协议路由、邮件、AI 资讯、设置、书签、笔记、代码片段、快速启动、游戏，或需要修改页面组件、Tauri 命令、Rust 模块入口时使用。修改前先从导航表定位准确文件，避免宽泛搜索。
---

# OneSpace 代码导航

## 立即定位

本 skill 加载后，先用用户输入匹配下表并直接打开列出的文件。存在匹配行时，不要先做宽泛 grep/glob。

| 业务关键词 | 前端页面/组件 | 前端功能模块 | 后端入口 (Tauri 命令模块) |
|---|---|---|---|
| 快速启动、Launcher、启动项 | `src/components/Launcher.tsx` | `src/lib/externalActions.ts` | `src-tauri/src/app_store/`（`launcher_*` 命令） |
| 工作空间、Workspace | `src/components/Workspaces/index.tsx` | `src/components/Workspaces/` | `src-tauri/src/workspaces.rs` |
| AI 会话、Session、终端会话 | `src/components/AiSessions.tsx` | `src/components/AiSessionsList.tsx` | `src-tauri/src/ai_sessions/` |
| AI 环境、CLI 环境、服务商 | `src/components/AiEnvironments/index.tsx` | `src/components/AiEnvironments/` | `src-tauri/src/ai_env/commands.rs` |
| AI 用量统计、Usage Stats | `src/components/AiUsageStats.tsx` | — | `src-tauri/src/ai_sessions/usage.rs` |
| AI Flow、流程状态 | `src/components/AiFlow/index.tsx` | — | `src-tauri/src/ai_flow.rs` |
| AI 工作台、AI Workspace、AI 对话 | `src/components/SmartWorkspaceHub.tsx` | `src/components/AiWorkspace/` | `src-tauri/src/ai_assistant/commands.rs` |
| Skills | `src/components/Skills/index.tsx` | — | `src-tauri/src/skills/commands.rs` |
| Subagents | `src/components/Subagents/index.tsx` | — | `src-tauri/src/subagents/commands.rs` |
| MCP Servers | `src/components/MCPServers/index.tsx` | — | `src-tauri/src/mcp_servers/commands.rs` |
| SSH 服务器 | `src/components/SshServers.tsx` | — | `src-tauri/src/ssh_oauth.rs` 中 `connect_ssh` / `get_ssh_hosts` |
| SSH 隧道、SSH Tunnels | `src/components/SshTunnels.tsx` | `src/components/sshTunnels/` | `src-tauri/src/ssh_tunnels/commands.rs` |
| 工作流、Workflow、Preset | `src/components/WorkflowPresetsPanel.tsx` | `src/components/RecentWorkflowRuns.tsx` | `src-tauri/src/workflows/` |
| 协议路由、Protocol Router | `src/components/ProtocolRouterTool.tsx` | `src/lib/protocolRouter.ts` | `src-tauri/src/protocol_router/commands.rs` |
| 设置、Settings、偏好 | `src/components/SettingsView.tsx` | — | `src-tauri/src/config.rs` |
| 邮件、Gmail、Mail | `src/components/Mail.tsx` | `src/lib/gmail.ts` | `src-tauri/src/ssh_oauth.rs`（OAuth）+ `src-tauri/src/app_store/` |
| AI 资讯、AI News、RSS | `src/components/AiNews.tsx` | — | `src-tauri/src/ai_news.rs` |
| 全局搜索、OmniSearch | `src/components/OmniSearch.tsx` | — | — |
| 书签、Bookmarks | `src/components/Bookmarks.tsx` | — | `src-tauri/src/storage.rs`（`read_bookmarks`/`save_bookmarks`） |
| 笔记、Notes | `src/components/Notes.tsx` | — | `src-tauri/src/storage.rs`（`read_notes`/`save_notes`） |
| 代码片段、Snippets | `src/components/Snippets.tsx` | — | `src-tauri/src/storage.rs`（`read_snippets`/`save_snippets`） |
| 云盘、Cloud Drive | `src/components/CloudDrive.tsx` | — | — |
| 游戏、Fish Pond、小游戏 | `src/components/FishPond.tsx` | `src/components/Games/` | `src-tauri/src/storage.rs`（`read_game_data`/`save_game_data`） |
| 文档、应用内文档、Documentation | `src/components/Documentation.tsx` | — | — |
| 新手引导、Onboarding | `src/components/OnboardingWizard.tsx` | — | `src-tauri/src/config.rs`（`should_show_onboarding`） |
| 消息中心、Message | `src/components/MessageCenter.tsx` | `src/lib/messages.ts` | `src-tauri/src/messages.rs` |
| 应用更新、Updater、升级 | `src/components/UpdateUpgradeModal.tsx` | `src/lib/updater.ts` | `src-tauri/src/app_runtime/run_app.rs`（updater plugin） |
| 备份、Backup | `src/components/BackupManager.tsx` | — | `src-tauri/src/backup.rs` |
| 主题、Theme、外观 | `src/components/ThemeProvider.tsx` | — | — |
| 快速 AI 条、Quick AI | `src/components/QuickAiSessionBar.tsx` | — | — |
| AI 助手小窗、Quick Assistant | `src/components/QuickAssistantWindow.tsx` | — | — |
| 模型中心、Model Center | `src/components/ModelCenter.tsx` | — | `src-tauri/src/ai_assistant/providers.rs` |

## 必须遵循的流程

1. 如果项目存在持久化记忆或 agent 指令，改代码前先读取。
2. 将用户术语匹配到 `立即定位` 表。
3. 如果匹配成功，直接读取列出的文件。
4. 如果没有匹配行，再读取相关 reference：
   - 前端修改：`references/frontend-navigation.md`
   - 后端/服务修改：`references/backend-navigation.md`
   - 功能归属确认：`references/feature-navigation.md`
5. 只有导航文件仍无法定位入口时，才使用聚焦搜索。

## 导航维护

- 新增功能入口时，必须同步更新 `立即定位` 和 `references/feature-navigation.md`。
- 前端文件移动、路由变化、feature module 变化时，必须更新 `references/frontend-navigation.md`。
- 后端文件移动、Tauri 命令入口、Rust 模块职责变化时，必须更新 `references/backend-navigation.md`。
- 导航表中每个路径都必须真实存在，除非明确标注为计划中或生成期路径。
