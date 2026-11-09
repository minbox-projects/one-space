# OneSpace

OneSpace 是一个面向开发者的桌面工作台（Tauri 2 + React 19 + TypeScript），把 AI CLI 环境、原生终端会话、AI 网关、云端智能体、MCP、Skills/Subagents 和常用开发工具收拢到一个窗口。提供 macOS（Intel / Apple Silicon）与 Windows x64 构建，界面支持中英双语。

## 下载与安装

从 [GitHub Releases](https://github.com/minbox-projects/one-space/releases) 下载对应平台的安装包。首次启动会进入初始化向导，选择数据存储方式（`local` / `iCloud` / `Git`）并设置主密码。

如果 macOS 提示「OneSpace 已损坏」，通常是 Gatekeeper 拦截：

```bash
sudo xattr -cr /Applications/OneSpace.app
```

## 快速上手

1. 在「AI 终端服务商」确认目标 CLI 的安装与版本，导入系统现有配置或新建环境，保存后点击 `Apply to CLI` 激活。
2. 在「AI 终端会话」选择工具与工作目录创建会话，OneSpace 会在原生终端中启动对应 CLI。
3. 按需在「Skills」「Subagents」「MCP Servers」安装扩展，并在「工作空间」中把它们绑定到项目目录。
4. 在「AI 终端会话」右上角点击 `Install CLI` 安装 `onespace` 命令（默认 `~/.local/bin/onespace`），即可在终端中创建与恢复会话。

## 功能概览

### 工作台

- **启动台（Launcher）**：统一启动应用、脚本、URL、文件夹或跳转应用内部页面；支持搜索、置顶、脚本信任确认与 JSON 导入导出。
- **工作空间（Workspaces）**：把项目目录与 AI 会话、MCP、Skills、Subagents 绑定在一起（四个页签），支持标签筛选与项目范围内的能力复制。

### AI 能力

**AI 终端服务商**

- 支持 `Claude`、`Codex`、`Antigravity`、`OpenCode`，自动检测本机 CLI 安装状态与版本。
- 可从系统现有配置或其它已同步设备导入默认环境。
- 多环境预设：`Save` 保存到 OneSpace，`Apply to CLI` 设为活动环境并写入 CLI 配置；`Env Managed` 决定 CLI 配置是否持续由 OneSpace 接管。
- 支持环境 JSON 导入导出，导入前预览冲突并逐条选择覆盖或新建。
- `OpenCode` 提供 provider JSON 编辑与模型快捷表单双向同步，保存保留历史版本、可回滚。

**AI 终端会话**

- 选择工具与工作目录即可在原生终端创建会话，会话可恢复、重命名、删除、复制 ID。
- 支持按工具/模型筛选与按名称搜索，标题与模型从各 CLI 历史自动回填。
- Quick AI Session Bar：全局快捷键唤起（默认 `Alt+Shift+A`），选择工具与目录后 `Enter` 直接启动。

**AI 云端智能体（Cloud Agent）**

- 在云端 SSH 主机上创建 Git Worktree 隔离任务，长程编程断网保活。
- 从本地终端一键直连或恢复云端会话，查看历史工作区。
- 监控 GitHub Issues 触发标签任务，自动实施并创建 PR（Issues 流水线）。
- 将本地 AI 网关配置同步到云端 Linux 网关。

**AI 网关**

- 把多个上游服务商聚合成本地统一入口，默认监听 `127.0.0.1:17688`（开发构建 `17689`），本地地址为 `http://127.0.0.1:<端口>/v1`。
- 上游支持 Chat Completions / Responses，并可原生转发 Anthropic Messages（`/v1/messages`）；可逐条映射覆盖协议。
- 每个上游使用有序密钥池；鉴权失败或额度耗尽的密钥会被标记，可单独重新启用。
- 模型映射可配置输入/缓存读/缓存写/输出四档单价与 UTC+8 峰谷时段，金额在请求时固化。
- **模型池**：把一个公共模型 id 分配给多个候选来源，按权重调度，并可为候选声明推理策略；模型池会作为独立模型出现在发现列表并同步到终端。
- 按权重轮询并保持同一会话的粘性；连续失败的映射自动禁用，冷却后半开探测恢复。
- 终端集成可把本地地址与默认 Key 一键配置到 `OpenCode`、`Codex`、`Claude`，创建独立的 `AI Gateway` 记录，不改写你已有的配置。
- 内置「用量统计」与「请求日志」：支持今天/昨天/近 7/15/30 天/全部，按模型、服务商或 UTC+8 自然日分组与过滤。

**AI 用量统计**

- 从本地 CLI 会话历史统计 token 用量，不请求云端账单，覆盖 `Claude`、`Codex`、`Antigravity`、`OpenCode`。
- 提供近 7 / 15 / 30 天窗口与每日趋势，展示 sessions / calls / total tokens / cache hit。
- `Antigravity` 不在磁盘持久化 token，token 列会显式标记为不可用。

### AI 扩展

**Skills**

- `Recommended` / `Repository` / `Installed` 三视图。
- 支持 `Global`（`~/.agents/skills`）与 `Project`（项目目录）两种安装范围，并自动发现四个工具的全局目录。
- 支持 Git 源同步、差异预览、更新应用、本地目录导入与打开。

**MCP Servers**

- 手动新增 `stdio` / `http` / `sse` 三类 Server，内置 GitHub、Filesystem、PostgreSQL、Context7、Brave Search、Slack、Google Maps、Puppeteer、Playwright、Figma、Weather 等模板。
- 支持按模型启用/禁用、链接到环境、导入导出、刷新本地安装状态，以及对部分 `npx` 型 Server 的更新检查。
- 页面内提供配置备份的创建、恢复、删除与清理。

**Subagents**

- 与 Skills 类似的三视图、安装范围与源管理。
- 提供源诊断：检查 frontmatter 缺失、`name` 缺失或非法、文件读取失败。

### 工具

侧边栏与「更多工具」中的常用工具：

**网络与远程**

| 工具 | 用途 |
| --- | --- |
| SSH 服务器 | 读取 `~/.ssh/config`，维护历史、忽略、收藏与自定义连接（密码或私钥）。 |
| SSH 隧道 | Local / Remote / Dynamic 三种转发，连接测试、自动连接与断线重连。 |
| 快速 SFTP | 浏览远程目录、预览与下载文件，流式跟踪远端日志。 |
| 服务器监控 | 通过 SSH 采集 Linux 主机的 CPU、内存、磁盘与网络使用情况。 |
| 内网穿透 | 通过 Cloudflare Quick Tunnel 或自建 FRP 把本机端口映射到公网。 |
| 文件共享 | 在可信局域网内以只读 HTTP 链接或二维码分享文件，可选访问密码与公网链接。 |
| SSL 证书 | 免费申请域名证书、TLS 握手与证书链检测、到期监控。 |
| 协议路由 | 为 Claude Profile 与 OpenAI 兼容供应商暴露本地 Anthropic 兼容入口。 |
| 服务状态监控 | HTTP / TCP / Ping / DNS 探活、心跳柱状图、SLA 统计、事件流与桌面/Webhook 告警。 |
| Webhook 捕获与重放 | 本地私密接收第三方回调，检视 Payload 与签名，一键重放到本地端口。 |

**调试与数据**

| 工具 | 用途 |
| --- | --- |
| DNS 解析与网络探测 | 多公共 DNS 实时比对、全记录解析、IP 地理与 ASN 情报、TCP 握手时延。 |
| URL 嵌套解构器 | 剖析超长 URL、自动解包嵌套回调、参数表格双向同步与代码导出。 |
| WebSocket & SSE | 监听大模型流式事件、双向 WebSocket 交互、心跳保活与时间轴排查。 |
| Curl 调试台 | 解析、调试并重放 HTTP cURL 请求，导出多语言代码。 |
| JWT 检查 | 离线解析与校验 JWT 载荷、声明。 |
| 其它 | JSON 解析、MD5 加密、随机密码、JT/T 808 / 809 / 1078 报文解析、TinyURL 短链接。 |

**本地工具**

- **备忘录 / 代码片段 / 收藏夹**：本地内容工具，参与侧边栏计数、OmniSearch 与内容同步。
- **AI Workflow 模型切换**：按 9×3 角色矩阵编辑并保存/激活 profile，模型候选自动从已发现的模型源读取。
- **蓝牙锁屏（仅 macOS）**：监测一个自选 BLE 设备，靠近唤醒或解锁，持续远离后锁定、息屏并可选暂停媒体；登录密码仅存本机 Keychain，配置不参与同步，自动解锁需授予辅助功能权限。
- **鱼塘**：主界面底部的小游戏集合（CyberMuyu、Snake、Tetris、Sudoku、Minesweeper、Wordle）。

### 通知与资讯

- **AI 新闻**：从自定义 RSS 源抓取资讯，内置 36Kr 与开源中国，支持关键词过滤、自动同步与保留策略。
- **邮件**：通过 Google OAuth 连接 Gmail（需自备 Client ID / Secret），支持收件箱、未读、正文与附件、发信与快速回复。
- **消息中心**：托管服务状态、备份结果、网关密钥标记等通知。

### 全局能力

- **OmniSearch（`Cmd/Ctrl + K`）**：聚合搜索会话、启动项、SSH、代码片段、书签、备忘录与 Skills。
- **托盘与全局快捷键**：主窗口显示/隐藏（默认 `Alt+Space`）与 Quick AI Session（默认 `Alt+Shift+A`）均可自定义；托盘菜单可打开主要页面，并为网关、协议路由、SSH 隧道与文件共享提供服务状态与批量操作。
- **中英双语**：语言与主题在「外观与语言」中切换；主窗口关闭默认隐藏到托盘。

## 设置与数据

设置页按分类独立保存与重置，当前分类包括：

| 分类 | 子页签 |
| --- | --- |
| 常规与系统 | 常规、外观与语言、快捷键、软件更新 |
| AI 与扩展生态 | 终端与模型、Skills 源、Subagents 源 |
| 网络与连接 | 网络代理、协议路由、AI 网关 |
| 运维与自动化 | SSL 证书、服务器监控、AI 云端智能体、Telegram 远程工作台 |
| 数据与安全 | 存储与多端同步、主密码与加密 |
| 资讯与订阅 | 资讯设置 |

数据采用 local-first 策略：运行时读写本地镜像，再按同步策略同步到 `local` / `iCloud` / `Git`，范围包括 providers、MCP、内容、Skills/Subagents 源与仓库、AI 新闻等。

**Telegram 远程工作台**：在「设置 → 运维与自动化」中配置 Bot Token 与 OpenCode 版本后，可在手机上触发 Planning 需求规划、Ask Question 交互决策，并让 OpenCode 执行与测试；支持进度卡片、Diff 预览、Git 上下文与命令菜单。

## 文档

- 使用手册：[`docs/USAGE.md`](./docs/USAGE.md)
- CLI 文档：[`docs/CLI.md`](./docs/CLI.md)
- Skills 与 Subagents：[`docs/SKILLS.md`](./docs/SKILLS.md)
- MCP：[`docs/MCP.md`](./docs/MCP.md)

## 技术栈

- **桌面框架**：Tauri 2（Rust）
- **前端**：React 19、TypeScript、Vite 7、Tailwind CSS 3、Radix UI、i18next
- **平台**：macOS 优先，提供 Intel / Apple Silicon 与 Windows x64 构建；会话、应用启动与 SSH 等能力依赖系统原生终端与 `open` / AppleScript 工作流

## 开发

需要 Node.js、Rust 与 Tauri CLI。构建内置的 OpenCode Zen sidecar 还需要 `cmake` 与 `ninja`（macOS：`brew install cmake ninja`，Windows：`choco install cmake ninja`）。

```bash
npm install
npm run tauri dev
```

`npm run tauri dev` 使用独立的 `~/.config/onespace-dev` 应用目录，与已安装的 release 应用（`~/.config/onespace`）互不共享可变状态。

| 命令 | 说明 |
| --- | --- |
| `npm run dev` | 仅启动 Vite 前端开发服务器 |
| `npm run tauri dev` | 启动完整桌面应用（开发模式） |
| `npm run tauri build` | 构建桌面应用 |
| `npm run lint` | ESLint 检查 |
| `npm test` | 前端测试（Vitest） |
| `npm run check:cli-matrix` | CLI 能力矩阵校验 |

后端测试：`cargo test --manifest-path src-tauri/Cargo.toml`。

版本号在 `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` 四处保持一致。

## Beta 版本

维护者可用 `python3 skills/onespace-release/scripts/release_tool.py beta <base>` 生成 `0.1.58-beta.1`、`0.1.58-beta.2` 这类版本，提交后推送 `v<base>-beta.N` 标签触发发布。Windows Beta 只生成 NSIS 安装包；稳定版同时生成 MSI 与 NSIS。

用户可在「设置 → 软件更新」开启「接收 Beta 更新」。Beta 与稳定版共用同一份配置与数据，该偏好仅保存在本机、默认关闭。
