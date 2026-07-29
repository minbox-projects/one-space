# OneSpace

OneSpace 是一个面向开发者的 macOS 桌面工作台，用来把 AI CLI、环境配置、MCP、Skills/Subagents、工作流和常用生产力工具收拢到一个窗口里。

当前实现重点是：

- 统一管理 `Claude`、`Codex`、`Gemini`、`OpenCode` 的环境预设与 CLI 配置
- 在原生终端中创建和恢复 AI 会话，并把会话记录同步回应用
- 以模型维度管理 `Skills`、`Subagents` 和 `MCP Servers`
- 通过工作流预设把目录、环境、MCP、Skills 一次性组合起来启动
- 提供 `Workspaces`、`AI Workspace`、`AI Usage Stats`、`AI Flow`、`Launcher`、`SSH Tunnels`、`Protocol Router`、`Snippets`、`Bookmarks`、`Notes`、`AI News`、`Gmail` 等配套工具

## 功能概览

### AI Environments

- 支持 `Claude`、`Codex`、`Gemini`、`OpenCode`
- 自动检测本机 CLI 是否已安装，并显示版本与安装指引
- 对 `Claude`、`Codex`、`Gemini` 支持从系统现有配置自动导入默认环境
- 支持多环境预设、激活当前环境、导出/导入环境 JSON
- 支持 `Env Managed` 开关，决定是否由 OneSpace 持续接管 CLI 配置文件
- 支持从其它已同步设备导入并激活环境

### AI Sessions And Workflows

- 从工作目录直接创建原生终端会话
- 会话可恢复、重命名、删除、复制 ID
- 会话名称和模型信息会持续从各 CLI 历史记录回填
- 支持 `Workflow Presets`
- 工作流可绑定工具、目录、环境、MCP、Skills、启动提示词和 `Shared/Strict` 作用域
- 提供依赖检查、一键补依赖、最近运行记录、重放与失败恢复

### Workspaces And AI Workspace

- `Workspaces` 按项目目录组织会话、MCP、Skills 和 Subagents
- `AI Workspace` 提供应用内 AI 对话、助手预设和 Quick Assistant 入口
- `AI Usage Stats` 从本地 CLI 会话历史统计 token 用量
- `AI Flow` 发现和操作项目中的 `.ai-flow` 目录，支持安装检查、计划状态和会话启动

### Skills And Subagents

- `Recommended / Repository / Installed` 三视图
- 按模型安装，也支持按项目范围安装
- 支持本地目录导入、远端源同步、差异预览、更新应用、打开本地目录
- `Subagents` 与 `Skills` 共用相似的管理流，但会额外提供源诊断能力

### MCP Servers

- 手动新增 `stdio / http / sse` 三类 MCP Server
- 模板创建，内置 GitHub、Filesystem、PostgreSQL、Context7、Slack、Google Maps、Brave Search、Puppeteer、Figma、Weather 等模板
- 支持按模型单独启用/禁用
- 支持链接到环境、导入导出配置、刷新本地安装状态
- 对部分 `npx` 型 `stdio` MCP 提供更新检查与更新应用

### Developer Utilities

- `Launcher`：启动应用、脚本、URL、文件夹，或跳转应用内部页面
- `OmniSearch`：统一搜索会话、启动项、SSH、代码片段、书签、笔记、Skills、工作流
- `SSH`：读取 `~/.ssh/config`，维护历史、收藏、忽略列表和自定义连接
- `SSH Tunnels`：维护 local / remote / dynamic SSH 转发配置，支持测试、连接和自动重连
- `Protocol Router`：管理本地协议路由、route endpoint、连接测试和近期请求用量
- `File Sharing`：在可信局域网内通过临时 HTTP 链接或二维码分享多个本地文件
- `Snippets`、`Bookmarks`、`Notes`
- `AI News`：从用户配置的 RSS 源抓取 AI 资讯，设置页内置 `36Kr`、`开源中国` 推荐源，支持关键词过滤和保留策略
- `Mail`：通过 Google OAuth 连接 Gmail，查看收件箱、阅读邮件、回复、下载附件

### Fun And Zen

- `Fish Pond` 内置 `CyberMuyu`、`Snake`、`Tetris`、`Sudoku`、`Minesweeper`、`Wordle`
- 入口位于主界面底部鱼形图标，不是独立侧边栏页面

### Experimental Areas

- `Cloud Drive` 当前仍是实验性/模拟状态
- 目前主要完成了 token 保存、基础浏览器界面和示例文件列表流程
- 不应把它视为完整可用的阿里云盘客户端

## 当前实现特点

- macOS-first：会话、SSH、应用启动依赖原生终端和 `open`/AppleScript 工作流
- local-first：运行时读写以本地镜像为主，再按配置同步到 `local / iCloud / Git`
- 支持托盘菜单、全局快捷键、Quick AI Session 浮动条
- 设置页按分区保存，每个分区可以独立保存和重置

## 文档

- 使用手册：[`docs/USAGE.md`](./docs/USAGE.md)
- CLI 文档：[`docs/CLI.md`](./docs/CLI.md)
- Skills 与 Subagents 文档：[`docs/SKILLS.md`](./docs/SKILLS.md)
- MCP 文档：[`docs/MCP.md`](./docs/MCP.md)
- 应用内入口：侧边栏 `Documentation`

## 推荐上手顺序

1. 完成初始化向导，选择 `Local / iCloud / Git`，设置主密码。
2. 进入 `AI Environments`，确认 CLI 安装状态并导入或创建环境。
3. 在 `Settings -> AI Terminal` 配置默认目录、默认模型和各工具启动命令。
4. 在 `AI Sessions` 里先手动创建一个会话，再试一次 `Workflow Preset`。
5. 根据需要补充 `Skills`、`Subagents` 和 `MCP Servers`。
6. 安装 `onespace` CLI，开始在终端里创建会话。

## 开发

```bash
npm install
npm run tauri dev
```

构建：

```bash
npm run tauri build
```

技术栈：

- Tauri 2
- Rust
- React 19
- TypeScript
- Tailwind CSS
- Radix UI

## macOS 常见安装问题

如果 macOS 提示“`OneSpace` 已损坏”，通常是 Gatekeeper 拦截导致：

```bash
sudo xattr -cr /Applications/OneSpace.app
```

## 国际化

- 支持中文和英文界面
- 语言可在 `Settings -> Appearance` 中切换
