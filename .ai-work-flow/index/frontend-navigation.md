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
| `ai-routing-gateway` | `src/components/AiRoutingGateway/index.tsx` | 独立侧边栏功能模块（位于 AI 用量后、Skills 前）；不通过 More Tools 或 Launcher 进入 |
| `ai-news` | `src/components/AiNews.tsx` | 侧边栏 |
| `skills` | `src/components/Skills/index.tsx` | 侧边栏 |
| `subagents` | `src/components/Subagents/index.tsx` | 侧边栏 |
| `mcp-servers` | `src/components/MCPServers/index.tsx` | 侧边栏 |
| `ssh` | `src/components/SshServers.tsx` | 侧边栏 |
| `ssh-tunnels` | `src/components/SshTunnels.tsx` | 侧边栏 |
| `protocol-router` | `src/components/ProtocolRouterTool.tsx` | 侧边栏 |
| `md5-encryption` | `src/components/Md5EncryptionTool.tsx` | 唯一 MD5 工具 ID；`src/lib/navigation.ts` 将同名直接别名解析为 `more-tools` 详情；从 `src/components/MoreToolsHub.tsx` 卡片/详情或 `src/components/Launcher.tsx` 快捷工具进入；`src/App.tsx` 按进入来源返回 More Tools 或 Launcher 上下文 |
| `short-link` | `src/components/ShortLinkTool.tsx` | `tools` 导航分组下的 More Tools 详情；`src/lib/navigation.ts` 将稳定 ID 解析为 `more-tools` + `short-link`；从 `src/components/MoreToolsHub.tsx` 卡片或 `src/components/Launcher.tsx` 中英文搜索入口进入，`src/App.tsx` 提供标题/面包屑并按来源返回 More Tools 或 Launcher 上下文 |
| `file-sharing` | `src/components/FileSharingTool.tsx` | `src/lib/navigation.ts` 解析至 `more-tools` 详情；从 `src/components/MoreToolsHub.tsx` 或 `src/components/Launcher.tsx` 进入 |
| `snippets` | `src/components/Snippets.tsx` | 侧边栏 / `more-tools` |
| `notes` | `src/components/Notes.tsx` | 侧边栏 / `more-tools` |
| `more-tools` | `src/components/MoreToolsHub.tsx` | 侧边栏（含 `bookmarks`、`cloud`、`short-link` 等辅助工具） |
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
| AI 路由网关 | `src/components/AiRoutingGateway/`、`src/lib/aiRoutingGateway.ts` | `src/App.tsx` 直接挂载并传递 `isVisible`；稳定 ID `ai-routing-gateway` 仅作为独立侧边栏模块，不注册为 More Tools 或 Launcher 工具 |
| MD5 文本工具 | `src/components/Md5EncryptionTool.tsx`、`src/lib/md5.ts` | `src/components/MoreToolsHub.tsx` 提供卡片与详情分发，`src/components/Launcher.tsx` 提供快捷入口，`src/lib/moreToolPresentation.ts` 提供展示元数据；`src/components/SettingsView.tsx` 复用共享 `md5Hex`；`src/lib/launcherToolVisibility.ts` 的 `md5Encryption` 默认 `true`，读取旧配置时以默认结构为基底逐字段仅合并 boolean 值，保留其他显式偏好并补全缺失字段；`src/App.tsx` 保留 More Tools/Launcher 返回上下文 |
| 短链接工具 | `src/components/ShortLinkTool.tsx`、`src/lib/shortLink.ts`、`src/lib/shortLinkHistory.ts` | `src/components/MoreToolsHub.tsx` 提供卡片/详情分发和 Launcher 可见性开关，`src/components/Launcher.tsx` 提供中英文名称/简介搜索入口；`src/lib/navigation.ts`、`src/lib/moreToolPresentation.ts`、`src/lib/launcherToolVisibility.ts` 提供稳定 `short-link` ID、展示元数据和默认可见性；文案来自 `src/i18n.ts`；`src/App.tsx` 提供标题、面包屑及 More Tools/Launcher 返回上下文；`src/lib/shortLink.ts` 包装 IPC，`src/lib/shortLinkHistory.ts` 管理最多 50 条本地历史 |
| 文件共享 | `src/components/FileSharingTool.tsx` | `src/App.tsx` 负责 More Tools 返回语义；`src/lib/fileSharing.ts` 提供 IPC/事件包装，`navigation.ts`、`moreToolPresentation.ts`、`launcherToolVisibility.ts` 提供 `file-sharing`、`Share2` 卡片和默认可见性；运行态只来自后端临时内存 snapshot |
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
| 短链接 IPC | `src/lib/shortLink.ts`（TinyURL 配置状态、Token 保存/删除、短链接创建） |
| 短链接历史 | `src/lib/shortLinkHistory.ts`（明文 localStorage，最多 50 条） |
| 文件共享 IPC | `src/lib/fileSharing.ts` |
| MD5 文本摘要 | `src/lib/md5.ts`（`md5Hex`，纯前端 UTF-8 实现） |
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

Short Link 中英文名称、简介、凭据、结果、历史和错误文案统一定义在 `src/i18n.ts` 的 `shortLink*` keys。
