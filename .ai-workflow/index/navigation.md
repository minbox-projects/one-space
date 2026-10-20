# Feature navigation

| Feature | Owner | Entry | Responsibility | Locate |
| --- | --- | --- | --- | --- |
| app-shell | frontend | src/App.tsx | 应用外壳、侧边栏/更多工具导航解析（工具别名经工具箱注册表解析，无旧标签归一化）、全局 Provider、i18n 与错误边界；`src/lib/runtimeStatus.ts` 为每个 WebView 提供 gateway/router | ai-workflow context locate --feature app-shell --verify |
| toolbox-plugin-registry | frontend | src/toolbox/registry.ts | 工具箱唯一注册表与共享运行时：每个工具一个描述符（`ToolboxToolDescriptor`，`loadComponent` 为动态 import loader、静态导入仅保留元数据与 loader，`ToolboxNavigation | ai-workflow context locate --feature toolbox-plugin-registry --verify |
| tray-menu | frontend | src/lib/trayMenu.ts | 托盘原生菜单所有权：`buildTrayMenuModel` 结构/标签/启用勾选/快捷键提示（遵循已注册快捷键的语义，`Meta` 不是修饰键，非法或未命名值省略）、`applyTrayMenu` 原生应用（单条目 accelerator | ai-workflow context locate --feature tray-menu --verify |
| ai-environments | frontend | src/components/AiEnvironments/index.tsx | Claude / Codex / Antigravity / OpenCode 环境预设、服务商配置、模型映射与同步设备导入 | ai-workflow context locate --feature ai-environments --verify |
| ai-sessions | frontend | src/components/AiSessions.tsx | 从工作目录创建、恢复与列出原生终端 AI 会话：`sessionsCreate`/`sessionsLaunch` 等经 `src/lib/aiSessions.ts` 类型化封装调用后端共享 canonical 会话服务；New Sess | ai-workflow context locate --feature ai-sessions --verify |
| quick-ai-session | frontend | src/components/QuickAiSessionBar.tsx | 全局快捷键唤起的快速 AI 会话浮条 | ai-workflow context locate --feature quick-ai-session --verify |
| workspaces | frontend | src/components/Workspaces/index.tsx | 按项目目录组织会话、MCP、Skills 与 Subagents，并提供能力面板；集合/详情刷新共享同一份 `load_sessions_state` 解码快照，多个刷新事件合并为一次 reload 并把详情刷新放在同一任务内 | ai-workflow context locate --feature workspaces --verify |
| ai-usage-stats | frontend | src/components/AiUsageStats.tsx | 从本地 CLI 会话历史统计 token 用量；`src/lib/aiUsage.ts` 提供 typed wrappers 与 DTO（`sessionsUsageToolStats`/`sessionsUsageDayStats`/`s | ai-workflow context locate --feature ai-usage-stats --verify |
| mcp-servers | frontend | src/components/MCPServers/index.tsx | MCP Server 增删改、模板创建、按模型启停与配置导入导出 | ai-workflow context locate --feature mcp-servers --verify |
| skills | frontend | src/components/Skills/index.tsx | Recommended / Repository / Installed 三视图的 Skills 安装、同步与本地导入 | ai-workflow context locate --feature skills --verify |
| subagents | frontend | src/components/Subagents/index.tsx | Subagents 目录浏览、安装同步与源诊断 | ai-workflow context locate --feature subagents --verify |
| launcher | frontend | src/components/Launcher.tsx | 启动应用 / 脚本 / URL / 文件夹及内部页面跳转；Launcher 快速工具与内部工具箱目标（Notes/Snippets）由工具箱注册表派生（`listToolboxTools("launcher-quick")`、`listTo | ai-workflow context locate --feature launcher --verify |
| omni-search | frontend | src/components/OmniSearch.tsx | 统一搜索会话、启动项、SSH、代码片段、书签、笔记、Skills | ai-workflow context locate --feature omni-search --verify |
| ssh-servers | frontend | src/components/SshServers.tsx | 读取 ~/.ssh/config，维护历史、收藏、忽略列表与自定义连接 | ai-workflow context locate --feature ssh-servers --verify |
| ssh-tunnels | frontend | src/components/SshTunnels.tsx | local / remote / dynamic SSH 转发配置、常用端口快速选择与维护，测试、连接与自动重连 | ai-workflow context locate --feature ssh-tunnels --verify |
| protocol-router | frontend | src/components/ProtocolRouterTool.tsx | 本地协议路由、endpoint 管理、连接测试与近期请求用量 | ai-workflow context locate --feature protocol-router --verify |
| ai-gateway | frontend | src/components/AiGateway/index.tsx | 本地 OpenAI 兼容转发控制台（左侧 AI 能力菜单入口）：上游服务商与模型映射（映射可声明协议与可选的本地模型显示名 `display_name`，未声明协议时继承服务商协议；每条映射另有持久化 `enabled`（字段缺省（不出现） | ai-workflow context locate --feature ai-gateway --verify |
| file-sharing | frontend | src/components/FileSharingTool.tsx | 可信局域网内通过临时 HTTP 链接或二维码分享本地文件 | ai-workflow context locate --feature file-sharing --verify |
| short-link | frontend | src/components/ShortLinkTool.tsx | 长链接转短链、历史记录管理与错误码处理 | ai-workflow context locate --feature short-link --verify |
| ai-news | frontend | src/components/AiNews.tsx | 按用户配置的 RSS 源抓取 AI 资讯，关键词过滤与保留策略 | ai-workflow context locate --feature ai-news --verify |
| mail | frontend | src/components/Mail.tsx | 通过 Google OAuth 连接 Gmail，查看、回复邮件并下载附件 | ai-workflow context locate --feature mail --verify |
| snippets | frontend | src/components/Snippets.tsx | 代码片段记录与检索 | ai-workflow context locate --feature snippets --verify |
| bookmarks | frontend | src/components/Bookmarks.tsx | 书签收藏与检索 | ai-workflow context locate --feature bookmarks --verify |
| notes | frontend | src/components/Notes.tsx | 本地笔记记录与检索 | ai-workflow context locate --feature notes --verify |
| fish-pond | frontend | src/components/FishPond.tsx | 主界面底部入口的小游戏集合：CyberMuyu、Snake、Tetris、Sudoku、Minesweeper、Wordle | ai-workflow context locate --feature fish-pond --verify |
| json-parser | frontend | src/components/JsonParserTool.tsx | JSON 格式化与解析工具 | ai-workflow context locate --feature json-parser --verify |
| md5-tool | frontend | src/components/Md5EncryptionTool.tsx | MD5 摘要计算工具 | ai-workflow context locate --feature md5-tool --verify |
| random-password | frontend | src/components/RandomPasswordTool.tsx | 随机密码生成工具 | ai-workflow context locate --feature random-password --verify |
| jtt-data-parser | frontend | src/components/JttDataParserTool.tsx | JT/T 808 / 809 / 1078 报文解析、hex 处理与输入历史 | ai-workflow context locate --feature jtt-data-parser --verify |
| settings | frontend | src/components/SettingsView.tsx | 分区保存的应用设置：AI Terminal、Appearance、连接、备份与更新，以及独立分区 `ai-gateway`（配置 AI 网关请求日志保留天数（默认 90、范围 1–365，非法值拒绝保存，不含模型价格表）与模板自动刷新间隔（ | ai-workflow context locate --feature settings --verify |
| onboarding | frontend | src/components/OnboardingWizard.tsx | 初始化向导：存储位置选择与主密码设置 | ai-workflow context locate --feature onboarding --verify |
| terminal-permission | frontend | src/components/TerminalPermissionConfirmDialog.tsx | 终端权限模式确认对话框与权限模式规范化 | ai-workflow context locate --feature terminal-permission --verify |
| ai-workflow-model-switcher | frontend | src/components/AiWorkflowModelSwitcher/index.ts | AI Workflow subagent 角色模型切换工具箱工具：9×3 矩阵编辑（`model` 与 `reasoning_effort`）与按本地声明能力感知的推理强度编辑——已知模型只提供 `ModelCapability` 声明 ∩ | ai-workflow context locate --feature ai-workflow-model-switcher --verify |
| ai-workflow-backend | backend | src-tauri/src/ai_workflow_profiles.rs | AI Workflow profile 与模型源后端：规范化 profile 矩阵，严格校验并原子保存 YAML；save-only 保存不运行 CLI 或改变 active profile，激活由独立命令执行且激活失败不丢弃已保存的 pr | ai-workflow context locate --feature ai-workflow-backend --verify |
| ai-env-backend | backend | src-tauri/src/ai_env.rs | 环境预设持久化、应用到 CLI 配置、模型列表抓取；legacy providers 回退经 `legacy_providers_path_at(app_dir)` 解析在 profile 应用目录，dev 不读取 release 的 `a | ai-workflow context locate --feature ai-env-backend --verify |
| ai-sessions-backend | backend | src-tauri/src/ai_sessions.rs | 会话存储、历史回填、终端创建与用量解析；`usage_cache.rs` 为四个工具提供 L1 整份 `ToolScan`（30s TTL、按解析来源根键控、held-mutex 合并）与 L2 单来源解析结果（path+len+mtime | ai-workflow context locate --feature ai-sessions-backend --verify |
| app-store-backend | backend | src-tauri/src/app_store.rs | 统一存储引擎、迁移、provider 投影、会话与 launcher 命令；`storage_engine` 的 `CanonicalStateWriteLease` 是 providers/sessions 及其元数据的单一跨进程事务门（ | ai-workflow context locate --feature app-store-backend --verify |
| config-backend | backend | src-tauri/src/config.rs | 应用配置、运行时 profile、Claude 多 profile、加密与密钥存储；按构建 profile 解析应用目录（`is_dev_build` 仅 `cfg(all(debug_assertions, not(test)))` 为真 | ai-workflow context locate --feature config-backend --verify |
| mcp-backend | backend | src-tauri/src/mcp_servers.rs | MCP Server 配置、模板、导出与运行时进程管理 | ai-workflow context locate --feature mcp-backend --verify |
| skills-backend | backend | src-tauri/src/skills.rs | 统一 ~/.agents/skills 目录初始化、迁移与冲突备份、Claude 兼容符号链接、目录扫描、仓库同步、安装与兼容矩阵；本地缓存基根经 `skills_local_cache_base_root_at(app_dir)` 解析在 | ai-workflow context locate --feature skills-backend --verify |
| subagents-backend | backend | src-tauri/src/subagents.rs | Subagents 源扫描、frontmatter 解析、仓库同步与安装；本地缓存基根经 `subagents_local_cache_base_root_at(app_dir)` 解析在 profile 应用目录（dev 为 `~/.co | ai-workflow context locate --feature subagents-backend --verify |
| workspaces-backend | backend | src-tauri/src/workspaces.rs | 工作区数据读写与能力关联；会话查询复用 `app_store` 的共享解码 sessions 快照，不重复解密整个会话状态 | ai-workflow context locate --feature workspaces-backend --verify |
| ssh-tunnels-backend | backend | src-tauri/src/ssh_tunnels.rs | SSH 转发配置、常用端口管理、运行时桥接与持久监管（持久重试监管循环、类型化终态分类、双步往返健康探测、desired-running 看门狗与启动认领、唤醒/窗口显示重试 poke）、配置校验与全量隧道批量连接/断开（批次标识 `all | ai-workflow context locate --feature ssh-tunnels-backend --verify |
| protocol-router-backend | backend | src-tauri/src/protocol_router.rs | 协议转换、SSE 响应、HTTP 运行时与请求统计 | ai-workflow context locate --feature protocol-router-backend --verify |
| ai-gateway-backend | backend | src-tauri/src/ai_gateway.rs | 独立加密配置存储、上游服务商支持权重配置（`weight: u32`，范围 1–100，默认 1，旧配置缺省为 1，超界拒绝写入）、按映射协议（未声明时继承服务商协议）筛选候选并通过平滑加权轮询（SWRR，`selection::weigh | ai-workflow context locate --feature ai-gateway-backend --verify |
| file-sharing-backend | backend | src-tauri/src/file_sharing.rs | 临时 HTTP 文件共享运行时与类型 | ai-workflow context locate --feature file-sharing-backend --verify |
| short-link-backend | backend | src-tauri/src/short_link.rs | 短链创建与配置状态 | ai-workflow context locate --feature short-link-backend --verify |
| ai-news-backend | backend | src-tauri/src/ai_news.rs | RSS 抓取、关键词过滤与保留策略执行 | ai-workflow context locate --feature ai-news-backend --verify |
| backup-backend | backend | src-tauri/src/backup.rs | 配置与数据备份、恢复 | ai-workflow context locate --feature backup-backend --verify |
| cli-tooling-backend | backend | src-tauri/src/cli_probe.rs | CLI 探测、更新检查、版本检测、git 命令与托管资源 | ai-workflow context locate --feature cli-tooling-backend --verify |
| app-runtime-backend | backend | src-tauri/src/lib.rs | Tauri 应用启动、CLI 入口、运行时服务（含进程级系统恢复信号：`SYSTEM_RESUME_GRACE_SECS = 60` 的 `mark_system_resume` / `system_resume_grace_active( | ai-workflow context locate --feature app-runtime-backend --verify |
| documentation | documentation-maintainer | docs/USAGE.md | 使用手册、CLI / MCP / Skills 文档、AI 网关性能基线与设计报告，以及应用内 Documentation 页面 | ai-workflow context locate --feature documentation --verify |
| cli-matrix-check | backend | tools/check-cli-matrix.sh | CLI 能力矩阵校验与网关脱敏校验脚本；`tools/measure-core-workflows.mjs` 用 node 内置模块运行 `core_workflows_perf` ignored 测试并解析 `CWF_METRIC` 行（ | ai-workflow context locate --feature cli-matrix-check --verify |
| ble-lock | frontend | src/components/BleLockTool.tsx | 低功耗蓝牙（BLE）锁屏工具箱（id `ble-lock`，别名 `ble-lock`）：只监测一个以 CoreBluetooth UUID 标识的设备（不解码 MAC、不推断距离），主动连接读 RSSI（默认）或被动广播两种模式；以解锁/ | ai-workflow context locate --feature ble-lock --verify |
| ble-lock-backend | backend | src-tauri/src/ble_lock.rs | BLE 锁屏后端（Rust）：`src-tauri/src/ble_lock.rs` 只声明 `mod config/state/central` 并 re-export 三个子模块公共类型；`config.rs` 严格校验并原子落盘当前  | ai-workflow context locate --feature ble-lock-backend --verify |
