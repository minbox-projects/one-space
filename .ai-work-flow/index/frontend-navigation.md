# OneSpace 前端结构导航

## 顶层结构

| 区域 | 用途 | 路径 |
|---|---|---|
| 应用入口 | 启动与 Provider 组装 | `src/main.tsx` |
| 应用外壳 | 路由、侧边栏、主题、布局、事件 | `src/App.tsx` |
| 全局样式 | Tailwind CSS 入口 | `src/index.css` |
| 国际化 | i18n 初始化 | `src/i18n.ts` |
| 测试 | 测试辅助与设置 | `src/test/` |
| 静态资源 | 图片、图标 | `src/assets/` |

## 侧边栏导航分组

定义在 `src/App.tsx` 的 `navigationGroups`（`core` / `capabilities` / `tools`），页面映射：

| 导航 ID | 前端组件 | 路由/进入方式 |
|---|---|---|
| `launcher` | `src/components/Launcher.tsx` | 默认首页 |
| `workspaces` | `src/components/Workspaces/index.tsx` | 侧边栏 |
| `ai-assistants` | `src/components/SmartWorkspaceHub.tsx` | 侧边栏（含子区 `conversations`、`assistants`、`automations`、`models`） |
| `ai-sessions` | `src/components/AiSessions.tsx` | 侧边栏 |
| `ai-environments` | `src/components/AiEnvironments/index.tsx` | 侧边栏 |
| `ai-usage` | `src/components/AiUsageStats.tsx` | 侧边栏 |
| `ai-news` | `src/components/AiNews.tsx` | 侧边栏 |
| `skills` | `src/components/Skills/index.tsx` | 侧边栏 |
| `subagents` | `src/components/Subagents/index.tsx` | 侧边栏 |
| `mcp-servers` | `src/components/MCPServers/index.tsx` | 侧边栏 |
| `ssh` | `src/components/SshServers.tsx` | 侧边栏 |
| `ssh-tunnels` | `src/components/SshTunnels.tsx` | 侧边栏 |
| `protocol-router` | `src/components/ProtocolRouterTool.tsx` | 侧边栏 |
| `ai-request-capture` | `src/components/AiRequestCaptureTool.tsx` | `src/lib/navigation.ts` 解析至 `more-tools` 详情；从 `src/components/MoreToolsHub.tsx` 或 `src/components/Launcher.tsx` 进入 |
| `snippets` | `src/components/Snippets.tsx` | 侧边栏 / `more-tools` |
| `notes` | `src/components/Notes.tsx` | 侧边栏 / `more-tools` |
| `more-tools` | `src/components/MoreToolsHub.tsx` | 侧边栏（含 `bookmarks`、`cloud`、`ai-request-capture`） |
| `documentation` | `src/components/Documentation.tsx` | 侧边栏 |
| `mail` | `src/components/Mail.tsx` | 侧边栏 |
| `fish-pond` | `src/components/FishPond.tsx` | 底部鱼形图标 |
| `settings` | `src/components/SettingsView.tsx` | 底部齿轮图标 |
| 独立视图（URL `?view=quick-ai`） | `src/components/QuickAiSessionBar.tsx` | URL 参数 |
| 独立视图（URL `?view=quick-assistant`） | `src/components/QuickAssistantWindow.tsx` | URL 参数 |
| 独立视图（URL `?view=selection-assistant`） | `src/components/QuickAssistantWindow.tsx`（`variant="selection"`） | URL 参数 |

## 功能模块入口

| 功能 | 模块路径 | 页面/主组件 |
|---|---|---|
| 工作空间 | `src/components/Workspaces/` | `src/components/Workspaces/index.tsx` |
| AI 环境 | `src/components/AiEnvironments/` | `src/components/AiEnvironments/index.tsx` |
| AI 工作台 | `src/components/AiWorkspace/` | `src/components/SmartWorkspaceHub.tsx` |
| Skills | `src/components/Skills/` | `src/components/Skills/index.tsx` |
| Subagents | `src/components/Subagents/` | `src/components/Subagents/index.tsx` |
| MCP Servers | `src/components/MCPServers/` | `src/components/MCPServers/index.tsx` |
| SSH 隧道 | `src/components/sshTunnels/` | `src/components/SshTunnels.tsx` |
| AI 请求抓包 | `src/components/AiRequestCaptureTool.tsx` | `src/App.tsx` 负责 More Tools 返回语义；`src/lib/navigation.ts`、`src/lib/moreToolPresentation.ts`、`src/lib/launcherToolVisibility.ts` 提供 ID、`ScanSearch` 卡片和 Launcher 默认可见性 |
| 游戏 | `src/components/Games/` | `src/components/FishPond.tsx` |
| UI 组件库 | `src/components/ui/` | — |

## 工具库入口

| 用途 | 路径 |
|---|---|
| 导航工具 | `src/lib/navigation.ts` |
| Tauri 外部 URL 打开 | `src/lib/externalActions.ts` |
| 用户行为日志 | `src/lib/userActions.ts` |
| 消息系统 | `src/lib/messages.ts` |
| Gmail API | `src/lib/gmail.ts` |
| 协议路由客户端 | `src/lib/protocolRouter.ts` |
| AI 请求抓包 IPC | `src/lib/aiRequestCapture.ts` |
| Skills 工具 | `src/lib/skills.ts` |
| Subagents 工具 | `src/lib/subagents.ts` |
| SSH 隧道工具 | `src/lib/sshTunnels.ts` / `src/lib/sshTunnelSummary.ts` / `src/lib/sshTunnelI18n.ts` |
| AI 工作台工具 | `src/lib/aiWorkspace.ts` / `src/lib/aiAssistant.ts` |
| 应用更新 | `src/lib/updater.ts` |
| 工作流工具 | `src/lib/workflows.ts` |
| 网络断路器 | `src/lib/networkCircuitBreaker.ts` |
| 系统事件构建器 | `src/lib/actionDescriptors/` |
| 终端权限 | `src/lib/terminalPermissions.ts` |
| 助手 MCP 展示 | `src/lib/assistantMcpDisplay.ts` |
| 助手工具调用 | `src/lib/assistantToolCalls.ts` |
| 通用工具 | `src/lib/utils.ts` |
