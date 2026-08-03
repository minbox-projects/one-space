# OneSpace 功能到入口总表

## 项目级入口

根 `MEMORY.md` 是项目级上下文与 Review Standards 的入口，记录领域术语、仓库约束、职责和模块边界。

| 功能 | 关键词 | 前端入口 | 后端/API 入口 | 备注 |
|---|---|---|---|---|
| 快速启动 | Launcher、启动项、快捷入口 | `src/components/Launcher.tsx` | `src-tauri/src/app_store/`（`launcher_*` 命令） | 含执行、信任、图标解析 |
| 工作空间 | Workspace、工作区 | `src/components/Workspaces/index.tsx` | `src-tauri/src/workspaces.rs` | 项目目录组织 |
| AI 会话 | AI Session、会话、终端 | `src/components/AiSessions.tsx` | `src-tauri/src/ai_sessions/` | 创建/恢复/删除 |
| AI 环境 | AI 环境、CLI、Claude、Codex、Gemini | `src/components/AiEnvironments/index.tsx` | `src-tauri/src/ai_env/commands.rs` | 环境预设/导入/激活 |
| AI 用量 | Usage Stats、用量统计、token | `src/components/AiUsageStats.tsx` | `src-tauri/src/ai_sessions/usage.rs` | Token 统计 |
| AI 工作台 | AI Workspace、AI 对话、助手 | `src/components/SmartWorkspaceHub.tsx` | `src-tauri/src/ai_assistant/commands.rs` | 含对话/助手库/自动化/模型中心 |
| AI 对话 | Conversation、对话、聊天 | `src/components/AiWorkspace/AiWorkspaceSimple.tsx` | `src-tauri/src/ai_assistant/conversations.rs` | 含上下文/发送 |
| 助手库 | Assistant、助手预设、提示词 | `src/components/AiWorkspace/` | `src-tauri/src/ai_assistant/settings.rs` | 管理提示词与模型绑定 |
| 自动化 | Automation、后台任务、定时 | `src/components/Schedules.tsx` | `src-tauri/src/ai_assistant/schedules.rs` | 触发器与运行记录 |
| 模型中心 | Model、模型目录、连接 | `src/components/ModelCenter.tsx` | `src-tauri/src/ai_assistant/providers.rs` | 模型连接/角色绑定 |
| Skills | Skill、技能 | `src/components/Skills/index.tsx` | `src-tauri/src/skills/commands.rs` | 安装/同步/更新 |
| Subagents | Subagent、子代理 | `src/components/Subagents/index.tsx` | `src-tauri/src/subagents/commands.rs` | 安装/同步/诊断 |
| MCP Servers | MCP、工具、stdio、http、sse | `src/components/MCPServers/index.tsx` | `src-tauri/src/mcp_servers/commands.rs` | 含模板/导入/导出 |
| SSH 服务器 | SSH、远程连接 | `src/components/SshServers.tsx` | `src-tauri/src/ssh_oauth.rs` | 读取 `~/.ssh/config` |
| SSH 隧道 | SSH Tunnel、端口转发 | `src/components/SshTunnels.tsx` | `src-tauri/src/ssh_tunnels/commands.rs` | local/remote/dynamic |
| 工作流 | Workflow、Preset、依赖检查 | `src/components/WorkflowPresetsPanel.tsx` | `src-tauri/src/workflows/` | 含启动/重放/依赖 |
| 协议路由 | Protocol Router、路由端点 | `src/components/ProtocolRouterTool.tsx` | `src-tauri/src/protocol_router/commands.rs` | 协议路由配置 |
| AI 路由网关 | AI Routing Gateway、SQLite、Keychain、账号池、OAuth 只读、模型映射、账号价格覆盖、Gateway Key 加密复制、多分组、时区统计、OpenAI Responses、Chat Completions | `src/components/AiRoutingGateway/`、`src/lib/aiRoutingGateway.ts`、`src/App.tsx`，稳定导航 ID 为 `ai-routing-gateway` | `src-tauri/src/ai_routing_gateway/commands/mod.rs`、`gateway_key.rs`、`accounts.rs`、`request_logs.rs`、`schema_v4.sql`、`src-tauri/src/shared_sqlite/migrations.rs`、`src-tauri/src/app_runtime/run_app.rs` | 独立侧边栏模块；账号与 AI 终端服务商不共享数据；API Key 账号可编辑连接、映射和四类价格，OAuth 仅展示且不注册新增/重登录 IPC；网关密钥完整值使用 RootKey 加密，列表前后各 6 位脱敏，复制走专用命令，分组即时保存，revoke 保留历史；今日/30 天按应用当前时区聚合；顶栏只在运行时显示图标 |
| 文件共享 | File Sharing、LAN、临时下载 | `src/App.tsx`、`src/lib/navigation.ts`、`src/components/MoreToolsHub.tsx`、`src/components/Launcher.tsx`、`src/components/FileSharingTool.tsx` | `src-tauri/src/app_runtime/run_app.rs`、`src-tauri/src/file_sharing.rs`、`src-tauri/src/file_sharing/{runtime,http,types}.rs` | nav/tool ID 为 `file-sharing`，通过 More Tools 或 Launcher 进入；可信私有 IPv4 HTTP 临时会话，仅内存保存令牌、文件和传输记录，应用退出清理；独立于 Cloud Drive、同步和备份 |
| MD5 文本工具 | MD5 Encryption、MD5 加密、文本哈希、`md5Hex` | `src/components/Md5EncryptionTool.tsx`、`src/components/MoreToolsHub.tsx`、`src/components/Launcher.tsx`、`src/lib/navigation.ts`、`src/App.tsx`；`src/components/SettingsView.tsx` 复用 `src/lib/md5.ts` | 无（纯前端） | 唯一 nav/tool ID 为 `md5-encryption`；共享 `src/lib/md5.ts` 的 `md5Hex` 按 UTF-8 计算文本摘要，SettingsView 复用同一实现；用户入口为 More Tools 卡片、Launcher 快捷工具和应用内直接别名；无后端能力、无网络调用、无托盘专属入口 |
| 短链接工具 | Short Link、生成短链接、TinyURL、历史 | `src/App.tsx`、`src/lib/navigation.ts`、`src/components/MoreToolsHub.tsx`、`src/components/Launcher.tsx`、`src/components/ShortLinkTool.tsx`、`src/lib/shortLink.ts`、`src/lib/shortLinkHistory.ts`、`src/lib/moreToolPresentation.ts`、`src/lib/launcherToolVisibility.ts`、`src/i18n.ts` | `src-tauri/src/short_link.rs`、`src-tauri/src/app_runtime/run_app.rs`、`src-tauri/src/secrets.rs` | 稳定 nav/tool ID 为 `short-link`；TinyURL API Token 使用专用 `tinyurl_api_token` secret 加密保存；本地历史以明文 localStorage 最多保留 50 条；删除本地记录不会删除、停用或撤销 TinyURL 远端短链接 |
| 设置 | Settings、偏好设置、配置 | `src/components/SettingsView.tsx` | `src-tauri/src/config.rs` | 分区保存 |
| 邮件 | Mail、Gmail、电子邮件 | `src/components/Mail.tsx` | 无独立后端模块 | Google OAuth 集成 |
| AI 资讯 | AI News、RSS、资讯 | `src/components/AiNews.tsx` | `src-tauri/src/ai_news.rs` | RSS 抓取与过滤 |
| 全局搜索 | OmniSearch、搜索 | `src/components/OmniSearch.tsx` | 前端-only | 统一搜索 |
| 书签 | Bookmarks、链接 | `src/components/Bookmarks.tsx` | `src-tauri/src/storage.rs` | 持久化书签 |
| 笔记 | Notes、记录 | `src/components/Notes.tsx` | `src-tauri/src/storage.rs` | 持久化笔记 |
| 代码片段 | Snippets、代码模板 | `src/components/Snippets.tsx` | `src-tauri/src/storage.rs` | 持久化片段 |
| 云盘 | Cloud Drive、云端文件 | `src/components/CloudDrive.tsx` | 实验性/模拟 | 非完整客户端 |
| 游戏 | Fish Pond、CyberMuyu、Snake | `src/components/FishPond.tsx` | `src-tauri/src/storage.rs`（游戏数据） | 内嵌小游戏合集 |
| 文档 | Documentation、帮助 | `src/components/Documentation.tsx` | 前端-only | 应用内文档 |
| 新手引导 | Onboarding、初始化向导 | `src/components/OnboardingWizard.tsx` | `src-tauri/src/config.rs` | 首次运行 |
| 消息中心 | Message、通知、消息 | `src/components/MessageCenter.tsx` | `src-tauri/src/messages.rs` | 系统消息 |
| 应用更新 | Updater、升级、更新 | `src/components/UpdateUpgradeModal.tsx` | Tauri updater plugin in `run_app.rs` | 自动更新 |
| 备份 | Backup、数据备份 | `src/components/BackupManager.tsx` | `src-tauri/src/backup.rs` | 创建/恢复/清理 |
| 主题 | Theme、外观、亮色/暗色 | `src/components/ThemeProvider.tsx` | 前端-only | 亮色/暗色/系统 |
| 快速 AI 条 | Quick AI、浮动条 | `src/components/QuickAiSessionBar.tsx` | 前端-only | 快捷 AI 入口 |
| AI 小窗 | Quick Assistant、selection | `src/components/QuickAssistantWindow.tsx` | 前端-only | 独立助手窗口 |
| MCP 模板 | MCP Template、模板创建 | 无独立组件 | `src-tauri/src/mcp_templates.rs` | 内置 MCP 模板 |
| MCP 导入导出 | MCP Export/Import、配置迁移 | `src/components/MCPImportExport.tsx` | `src-tauri/src/mcp_export.rs` | 导出/导入配置 |
| 代理 | Proxy、网络代理 | 设置在 `SettingsView.tsx` 中 | `src-tauri/src/proxy.rs` | 系统代理配置 |
| 密码 | Master Password、主密码 | 设置在 `SettingsView.tsx` 中 | `src-tauri/src/secrets.rs` | 加解密 |
| 同步 | Sync、Git 同步、iCloud | 状态在 `App.tsx` | `src-tauri/src/app_store/`（`sync_*` 命令） | 数据同步 |
| 国际化 | i18n、多语言、中英文 | `src/i18n.ts` + `public/locales/` | `src-tauri/src/config.rs`（存储语言设置） | 中/英 |
| 托盘菜单 | Tray、系统托盘 | `App.tsx` tray 事件处理 | `src-tauri/src/app_runtime/shortcuts_tray.rs` | 托盘交互 |
| 全局快捷键 | Shortcut、快捷键 | — | `src-tauri/src/app_runtime/shortcuts_tray.rs` | Alt+Space 等 |
