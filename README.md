# OneSpace

OneSpace 是一个面向开发者的 macOS 优先桌面工作台（Tauri 2 + React 19 + TypeScript），把 AI CLI 环境、原生终端会话、AI 网关、MCP、Skills/Subagents 和常用生产力工具收拢到一个窗口里。

- 统一管理 `Claude`、`Codex`、`Antigravity`、`OpenCode` 四种 AI CLI 的环境与配置
- 在原生终端中创建和恢复 AI 会话，标题与模型自动从 CLI 历史回填
- 用本地 AI 网关聚合多个上游服务商，为 `OpenCode` / `Codex` 提供统一入口
- 按项目组织会话、MCP、Skills 与 Subagents
- 附带 SSH、隧道、协议路由、文件共享、短链与离线小工具，支持中英双语界面

## 快速上手

1. 首次启动完成初始化向导：选择数据存储方式（`local` / `iCloud` / `Git`）并设置主密码。
2. 进入 `AI 终端服务商`，确认目标 CLI 的安装与版本，导入系统现有配置或新建环境，保存后 `Apply to CLI` 激活。
3. 进入 `AI 终端会话`，选择工具与工作目录创建会话，OneSpace 会在原生终端里启动对应 CLI。
4. 按需补充 `Skills`、`Subagents`、`MCP Servers`，并在工作空间中把它们绑定到项目目录。
5. 在 `AI 终端会话` 右上角点击 `Install CLI` 安装 `onespace` 命令行，即可在终端里创建与恢复会话。

## 功能概览

### 工作台

- **启动台（Launcher）**：统一启动应用、脚本、URL、文件夹或跳转应用内部页面；支持搜索、置顶排序、脚本信任确认、导入/导出 JSON。
- **工作空间（Workspaces）**：把项目目录与 AI 会话、MCP、Skills、Subagents 绑定在一起（Sessions / MCP / Skills / Subagents 四个页签），支持标签筛选与项目范围内的能力复制。

### AI 能力

**AI 终端服务商（AI Terminal Environments）**

- 支持 `Claude`、`Codex`、`Antigravity`、`OpenCode`，自动检测本机 CLI 安装状态与版本
- `Claude` / `Codex` / `Antigravity` 可从系统现有配置自动导入默认环境，并支持从其它已同步设备一键导入
- 多环境预设：`Save` 保存到 OneSpace，`Apply to CLI` 设为活动环境并写入 CLI 配置
- `Env Managed` 开关决定后续 CLI 配置文件是否持续由 OneSpace 接管
- 支持环境 JSON 导入导出，导入前可预览冲突并逐条选择覆盖或新建
- `OpenCode` 提供高级 provider JSON 编辑与模型快捷表单实时双向同步，保存保留历史版本、可回滚

**AI 终端会话（AI Terminal Sessions）**

- 选择工具与工作目录即可在原生终端创建会话
- 会话可恢复、重命名、删除、复制 ID，支持按工具/模型筛选与按名称搜索
- 会话标题与模型信息会从各 CLI 历史记录自动回填
- 内置 `Install CLI` 按钮安装 `onespace` 命令行（默认 `~/.local/bin/onespace`）
- Quick AI Session Bar：全局快捷键唤起（默认 `Alt+Shift+A`），选择工具与目录后 `Enter` 直接启动，`Esc` 关闭

**AI 网关（AI Gateway）**

- 把多个上游服务商聚合成本地统一入口，默认监听 `127.0.0.1:17688`（开发构建为 `17689`），本地 Api 地址（`http://127.0.0.1:<端口>/v1`）可直接复制给调用方
- 上游维护名称、Api 地址、密钥、默认模型、模型映射与接口协议（Chat Completions / Responses，可逐条映射覆盖）；支持服务商模板与模型清单同步
- 每个上游使用有序密钥池，密钥按列表顺序使用；鉴权失败与额度耗尽的密钥会被标记，可在界面中单独重新启用
- 模型映射可维护输入/缓存读/缓存写/输出四档单价与 UTC+8 峰谷优惠时段，金额在请求记录时固化
- `AI 终端集成` 可把本地 Api 地址与默认本地 Key 一键配置到 `OpenCode` / `Codex`（创建独立的 `AI Gateway` 服务商记录，不改写你已有的记录）
- 内置「用量统计」与「请求日志」页签：支持今日 / 昨天 / 近 7 天 / 近 15 天 / 近 30 天 / 全部范围，按模型、服务商或 UTC+8 自然日分组，并可按状态、模型、服务商过滤
- 连续失败的模型映射会被自动禁用，冷却后自动探测恢复；密钥被标记时会推送界面提示与消息中心通知

**AI 用量统计（AI Usage Stats）**

- 从本地 CLI 会话历史统计 token 用量，不请求云端账单接口，支持 `Claude`、`Codex`、`Antigravity`、`OpenCode`
- 提供 7 天 / 30 天 / 90 天等时间窗口与每日趋势，展示 sessions / calls / total tokens / cache hit
- `Antigravity` 不在磁盘持久化 token，token 列显示显式不可用说明

### AI 扩展

**Skills**

- `Recommended` / `Repository` / `Installed` 三视图
- 支持 `Global`（工具全局）与 `Project`（项目目录）两种安装范围
- 各工具统一使用 `~/.agents/skills` 规范目录，并为 `Claude` 维护兼容符号链接
- 支持 Git 源同步、差异预览、更新应用、本地目录导入与打开

**MCP Servers**

- 手动新增 `stdio` / `http` / `sse` 三类 Server
- 内置模板创建：GitHub、Filesystem、PostgreSQL、Context7、Brave Search、Slack、Google Maps、Puppeteer、Playwright、Figma、Weather 等
- 支持按模型单独启用/禁用、链接到环境、导入导出配置、刷新本地安装状态
- 对部分 `npx` 型 `stdio` MCP 提供更新检查与更新应用
- 页面内提供配置备份管理（创建、恢复、删除、清理旧备份）

**Subagents**

- 与 Skills 类似的三视图、安装范围与源管理
- 额外提供源诊断：检查 frontmatter 缺失、`name` 缺失或非法、文件读取失败

### 工具

侧边栏与「更多工具」（工具箱）中的常用工具：

- **SSH 服务器（SSH Servers）**：读取 `~/.ssh/config`，维护 history / ignored / 收藏与自定义连接（支持密码或私钥文件模式）
- **SSH 隧道（SSH Tunnels）**：维护 Local / Remote / Dynamic 三种端口转发，支持连接测试、自动连接、断线重连（网络恢复或系统唤醒后自动重试）、常用端口管理与环境分组
- **协议路由（Protocol Router）**：为 AI provider 暴露可复用的本地 endpoint，查看 route 状态、连接测试与近期请求用量；运行参数在设置页维护
- **文件共享（File Sharing）**：在可信局域网内通过临时 HTTP 链接或二维码分享本地文件，接收方只读下载；共享状态仅存在于当前进程，停止或退出后链接立即失效
- **生成短链接（Short Link）**：通过 TinyURL 生成短链（需在工具内填写 API Token），带本地历史记录
- **JSON 解析**、**MD5 加密**、**随机密码**、**JT/T 数据解析**（JT/T 808 / 809 / 1078 报文）
- **备忘录（Notes）**、**代码片段（Snippets）**、**收藏夹（Bookmarks）**：本地内容工具，参与侧边栏计数、OmniSearch 与内容同步
- **AI Workflow 模型切换**：按 9×3 角色矩阵编辑并保存/激活 profile，模型候选自动从已发现的模型源读取，也支持手动输入

### 通知与资讯

- **AI 新闻资讯（AI News）**：从自定义 RSS 源抓取 AI 资讯，内置 `36Kr`、`开源中国` 推荐源，支持关键词过滤、自动同步、保留天数与条数策略
- **邮件（Mail）**：通过 Google OAuth 连接 Gmail（需自备 Client ID / Secret），支持收件箱、未读状态、正文与附件下载、发信与快速回复
- **消息中心**：托管服务状态、备份结果、网关密钥标记等通知
- **鱼塘（Fish Pond）**：主界面底部入口的小游戏集合 — CyberMuyu、Snake、Tetris、Sudoku、Minesweeper、Wordle

### 全局能力

- **OmniSearch（`Cmd/Ctrl + K`）**：聚合搜索会话、启动项、SSH、代码片段、书签、备忘录与 Skills
- **托盘与全局快捷键**：主窗口显示/隐藏（默认 `Alt+Space`）与 Quick AI Session（默认 `Alt+Shift+A`）均可自定义；托盘菜单可显示/隐藏主窗口、唤起 Quick AI、打开各主要页面（含「更多页面」子菜单），并为网关、协议路由、SSH 隧道与文件共享提供服务状态与批量操作（连接/断开、停止共享、Sync Now、复制 API 地址），以及检查更新、打开设置与关于
- **中英双语界面**：语言与主题在「外观」中切换；主窗口关闭默认隐藏到托盘
- **应用内文档**：关于窗口与托盘菜单可进入内置文档

## 设置与数据

首次启动的初始化向导只做两件事：选择数据存储方式（`local` / `iCloud` / `Git`）与设置主密码（用于保护本地敏感信息，后续可在「安全」中修改）。

设置页按分区独立保存与重置，当前分区包括：

| 分区 | 主要配置 |
| --- | --- |
| 数据存储 | 存储类型、Git 地址与认证、iCloud / 本地路径、同步策略（providers、mcp、content、Skills / Subagents 源与仓库、AI 新闻等） |
| 新闻资讯 | AI News 自动同步、间隔、保留策略、关键词与 RSS 源列表 |
| 通用 | 开机自启动 |
| 更新 | 自动更新开关与检查间隔 |
| Skills 源 / Subagents 源 | 自动同步开关与间隔、Git 源清单、导入 / 导出、手动 Sync Now |
| 网络代理 | `http` / `https` / `socks5` 代理、账号密码、连通性测试 |
| 协议路由 | 启用开关、本地端口、请求记录保留天数、Router token |
| AI 网关 | 请求日志保留天数（默认 90 天）、服务商模板自动刷新间隔 |
| 快捷键配置 | 主窗口与 Quick AI Session 的全局快捷键 |
| AI 终端会话 | 默认工作目录、默认模型、终端应用、各模型启动命令模板与恢复会话权限模式 |
| 外观 | 语言（中文 / English）与主题 |
| 安全 | 查看 / 修改主密码，生成随机密码 |

## 文档

- 使用手册：[`docs/USAGE.md`](./docs/USAGE.md)
- CLI 文档：[`docs/CLI.md`](./docs/CLI.md)
- Skills 与 Subagents 文档：[`docs/SKILLS.md`](./docs/SKILLS.md)
- MCP 文档：[`docs/MCP.md`](./docs/MCP.md)

## 技术栈

- **桌面框架**：Tauri 2（Rust）
- **前端**：React 19、TypeScript、Vite 7、Tailwind CSS 3、Radix UI、i18next
- **平台**：macOS 优先；会话、应用启动与 SSH 会话等能力依赖 macOS 原生终端与 `open` / AppleScript 工作流

## 开发

```bash
npm install
npm run tauri dev
```

`npm run tauri dev` 使用独立的 `~/.config/onespace-dev` 应用目录，与已安装的 release 应用（`~/.config/onespace`）互不共享可变状态；首次 debug 启动会按条件从 release 目录播种 `.local_key` 与 `ai_gateway.json`（仅在 dev 目标缺失时复制、绝不覆盖），用量数据库不参与复制。dev 默认使用 local 存储，并使用自己的终端服务商标识 `gateway-dev` / `AI Gateway (Dev)`。

常用命令：

| 命令 | 说明 |
| --- | --- |
| `npm run dev` | 仅启动 Vite 前端开发服务器 |
| `npm run tauri dev` | 启动完整桌面应用（开发模式） |
| `npm run tauri build` | 构建桌面应用 |
| `npm run lint` | ESLint 检查 |
| `npm test` | 前端测试（Vitest） |
| `npm run check:cli-matrix` | CLI 能力矩阵校验 |

后端测试：`cargo test --manifest-path src-tauri/Cargo.toml`。

版本号在 `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 三处保持一致。

## macOS 常见安装问题

如果 macOS 提示「OneSpace 已损坏」，通常是 Gatekeeper 拦截导致：

```bash
sudo xattr -cr /Applications/OneSpace.app
```
