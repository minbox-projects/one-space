# OneSpace 使用手册

这份手册按「首次使用 -> 高频功能 -> 进阶配置」组织，适合新用户快速上手，也可作为日常查阅索引。

## 1. 首次启动（推荐流程）

1. 打开 OneSpace，进入初始化向导。
2. 选择数据存储方式：
   - `local`：本机存储，适合单机使用。
   - `icloud`：放在 iCloud Drive 目录，适合同 Apple 生态多设备同步。
   - `git`：使用 Git 仓库做版本化同步，适合团队或可追溯场景。
3. 设置主密码（Master Password），用于本地敏感数据加密。
4. 完成后进入主界面。

## 2. AI Environments（AI 环境管理）

### 2.1 支持工具

- Claude
- Codex
- Gemini
- OpenCode

### 2.2 核心概念

- `Preset / Provider`：一套可复用的 API Key、Base URL、Model 与高级参数。
- `Save`：保存当前配置。
- `Apply to CLI`：将当前配置写入对应 CLI 的本地配置文件并激活。

### 2.3 首次导入已有 CLI 配置

OneSpace 会在环境页自动探测并尝试导入系统已有配置（受支持工具）：

- Claude：`~/.claude/settings.json`
- Codex：`~/.codex/auth.json`、`~/.codex/config.toml`
- Gemini：`~/.gemini/.env`、`~/.gemini/settings.json`
- OpenCode：`~/.config/opencode/opencode.json`

### 2.4 常见操作

1. 切换左侧工具标签（Claude/Codex/Gemini/OpenCode）。
2. 选择已有环境或创建新环境。
3. 编辑参数后先点 `Save`，再点 `Apply to CLI`（OpenCode 按自身机制启用）。
4. 对 Claude/Codex/Gemini 可切换 `Env Managed`（环境托管）开关。

### 2.5 OpenCode 特有能力

- 以 JSON 方式直接编辑 provider 配置。
- 保存历史版本，可回滚并再次保存。
- 支持默认模型、默认 Agent、会话目录等全局配置。

## 3. AI Sessions（AI 会话）

### 3.1 创建会话

1. 进入 `AI Sessions`。
2. 点击 `New Session`。
3. 选择会话名、命令（Claude/Gemini/Codex/OpenCode/自定义）、工作目录。
4. 点击 `Launch`，会在原生终端启动。

### 3.2 管理会话

- `Continue`：恢复会话。
- `Rename`：重命名会话。
- `Delete`：删除会话记录。
- 复制 `Session ID`：用于排障或手动恢复。

### 3.3 快速会话条（Quick AI Session Bar）

- 默认全局快捷键：`Alt + Shift + A`
- 用于快速输入会话名并直接启动，无需回主界面。
- 可在设置中修改快捷键、默认目录、默认模型。

## 4. CLI 使用（命令行）

先在 `AI Sessions` 页面点击 `Install CLI`，默认安装到 `~/.local/bin/onespace`。

### 4.1 启动会话

```bash
onespace ai <模型简称> [会话名称]
```

模型简称：`claude` / `gemini` / `codex` / `opencode`

### 4.2 列出与切换环境

```bash
onespace env list
onespace env use <工具名称> <环境名称或ID>
```

详细示例见：[CLI 文档](./CLI.md)。

## 5. Skills（技能）与 Subagents（智能体）

### 5.1 概念与定位
- **Skills**：可复用的任务能力包，针对特定任务的提示词和执行流程，支持多模型。
- **Subagents**：特定目标的自治代理预设，常用于 Claude Agent 等高级场景。

详细技能文档：[`docs/SKILLS.md`](./SKILLS.md)

### 5.2 三种核心视图

无论是 Skills 还是 Subagents，都提供以下三种视图：
- `Recommended`：推荐视图（来自已配置的源仓库）
- `Repository`：仓库视图（包含远端同步和本地导入的内容）
- `Installed`：已安装视图（查看当前选中模型下已安装的项目）

### 5.3 核心操作

- `Sync Now`：手动同步源数据。
- 按模型安装（Claude/Gemini/Codex/OpenCode 可多选）。
- 本地目录导入（支持处理冲突：覆盖或跳过）。
- 更新管理：检查更新、预览差异（Diff）后应用更新。

### 5.4 源配置入口

在 `Settings -> Skills 源` 和 `Settings -> Subagents 源` 中：
- 添加/启用/禁用 Git 仓库源。
- 配置后台自动同步间隔。
- 导入/导出配置 JSON。

## 6. Workflow Presets（工作流预设）

为 AI 助手打造可复用的启动模板，一键加载所需的全部依赖环境。

### 6.1 核心配置
- **预设名称与目录**：指定工作流的标识和默认执行目录。
- **目标工具与模型**：选择启动哪种 AI 工具（如 Claude Code）并（可选）指定 Provider 预设。
- **关联依赖**：为该工作流指定所需的 **MCP Servers** 和 **Skills**。
- **启动作用域 (Launch Scope)**：
  - `Shared`：对该工具全局生效，MCP 与 Skills 将安装到该工具的主环境中。
  - `Strict`：隔离运行，仅为当前会话加载选定的 MCP 和 Skills。
- **启动提示词 (Launch Prompt)**：自动化执行的首条对话指令。

### 6.2 依赖检查与一键修复
选中工作流后，系统会自动检测环境依赖：
- 提示缺失的 MCP Server 或未激活的 MCP 链接。
- 提示缺失的 Skills。
- 提供 **一键修复 (One-click Fix)** 功能，自动安装和启用所有确实的依赖项。

## 7. MCP Servers

详细文档：[`docs/MCP.md`](./MCP.md)

### 7.1 能力范围

- 新增/编辑/删除 MCP Server
- 模板快速创建（如 GitHub / Filesystem / PostgreSQL 等）
- 导入导出 MCP 配置
- 为不同模型单独开关启用状态

### 7.2 推荐使用顺序

1. 创建或导入 MCP Server。
2. 关联到目标环境（Provider）。
3. 在模型视角确认已为对应模型启用。

## 8. 生产力模块与运维工具

### 8.1 开发者工具
- `SSH 管理`：从 `~/.ssh/config` 自动导入、管理历史、支持私钥。
- `Launcher`：快速启动应用/脚本/网址/文件夹。
- `Snippets`：代码片段分组、标签、语法高亮、一键复制。
- `Bookmarks` 与 `Notes`：支持 Markdown 的笔记和网址本地路径收藏。
- `OmniSearch`：聚合检索上述所有生产力资源与会话。

### 8.2 备份管理 (Backup Manager)
支持对各 AI CLI 的配置文件进行管理：
- **创建备份**：对当前的配置文件生成历史快照（并可填写备注）。
- **查看与恢复**：浏览历史备份文件的差异，并一键还原。
- **清理过期备份**：自动/手动清理 30 天前的旧备份，释放存储空间。

## 9. 游戏与解压 (Fun & Zen)

OneSpace 在生产力工具之外，提供了一系列经典游戏与心理卸压工具。

### 9.1 经典游戏 (Games)
在侧边栏进入 `Games` 模块：
- **扫雷 (Minesweeper)**、**贪吃蛇 (Snake)**、**数独 (Sudoku)**、**俄罗斯方块 (Tetris)**、**猜单词 (Wordle)**。

### 9.2 电子禅意 (Zen Tools)
- **电子木鱼 (CyberMuyu)** 与 **电子鱼缸 (FishPond)**：助您在编译等待时平复心情，提供宁静的桌面视觉背景。

## 10. Mail 与 Cloud

### 10.1 Gmail 邮件
- **OAuth 认证**：安全连接您的 Google 账号。
- 查看收件箱、阅读正文、下载附件并快速回复。实时通知侧栏未读邮件数量。

### 10.2 阿里云盘 (Cloud Drive)
- 通过 Refresh Token 连接，浏览目录、下载文件和预览图片。

## 11. Settings (设置)

### 11.1 数据与同步 (Data Storage)
- **存储类型**：Local / iCloud / Git
- **细粒度同步策略 (Sync Policy)**：如果启用 iCloud/Git，可精确控制同步哪些内容（例如：AI 环境、MCP、内容数据、工作流预设、Skills/Subagents 源配置与镜像仓库）。

### 11.2 外观、快捷键与语言
- **快捷键**：主窗口呼出（默认 `Alt+Space`）、快速 AI 会话（默认 `Alt+Shift+A`）。
- **语言与主题**：中英双语切换，亮暗色主题自适应。
- **开机自启**：配置 `Launch at Login`。

### 11.3 安全与代理
- **网络代理**：支持 HTTP/HTTPS/SOCKS5 及密码认证。
- **敏感数据加密**：修改主密码（Master Password）对本地敏感数据（如 API Key, Token）进行加密存储。

## 12. 常见问题

### Q1：终端提示找不到 `onespace` 命令

把 `~/.local/bin` 加入 `PATH`，例如：
```bash
export PATH="$HOME/.local/bin:$PATH"
```

### Q2：切换环境后 CLI 没生效

在 AI Environments 中确认：
1. 已 `Save` 并 `Apply to CLI`。
2. 对应工具 `Env Managed` 为开启（Claude/Codex/Gemini）。

### Q3：如何解决 macOS 提示 “OneSpace 已损坏”？

参考 README 中的「macOS 安装与运行」章节执行 `xattr` 修复命令。
