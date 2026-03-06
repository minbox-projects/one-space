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

## 5. Skills（技能）

详细文档：[`docs/SKILLS.md`](./SKILLS.md)

### 5.1 三种视图

- `Recommended`：推荐技能
- `Repository`：仓库镜像技能
- `Installed`：已安装技能

### 5.2 核心操作

- `Sync Now`：手动同步技能源
- 按模型安装（Claude/Gemini/Codex/OpenCode 可多选）
- 本地目录导入技能（支持冲突处理：覆盖/跳过）
- 查看更新差异后应用更新

### 5.3 技能源配置入口

在 `Settings -> Skills 源` 中：

- 添加/启用/禁用 Git 仓库源
- 配置自动同步间隔
- 导入/导出技能源 JSON

## 6. MCP Servers

详细文档：[`docs/MCP.md`](./MCP.md)

### 6.1 能力范围

- 新增/编辑/删除 MCP Server
- 模板快速创建（如 GitHub / Filesystem / PostgreSQL 等）
- 导入导出 MCP 配置
- 为不同模型单独开关启用状态

### 6.2 推荐使用顺序

1. 创建或导入 MCP Server。
2. 关联到目标环境（Provider）。
3. 在模型视角确认已为对应模型启用。

## 7. SSH 管理

### 7.1 视图

- `Config`：来自 `~/.ssh/config` 的主机
- `History`：连接历史
- `Ignored`：已忽略主机
- `Custom`：手动连接（密码/私钥）

### 7.2 实用功能

- 收藏常用主机（优先排序）
- 忽略低频主机
- 自动记录连接次数和最近连接时间

## 8. 生产力模块

- `Launcher`：快速启动应用/脚本/网址/文件夹
- `Snippets`：代码片段分组、标签、语法高亮、一键复制
- `Bookmarks`：网址与本地路径收藏，标签筛选
- `Notes`：Markdown 笔记，自动保存
- `OmniSearch`：聚合检索会话、SSH、Snippet、收藏、笔记、技能

## 9. Mail 与 Cloud

### Mail（Gmail）

- 使用 OAuth 连接 Gmail
- 收件箱、邮件详情、发送邮件、附件处理
- 支持读取未读数并同步到侧栏

### Cloud Drive（阿里云盘）

- 支持通过 Token 连接并浏览文件
- 当前实现以基础文件浏览流程为主，适合作为早期集成能力使用

## 10. Settings（设置）

### 10.1 数据与同步

- 存储类型：Local / iCloud / Git
- Git 模式下可配置 URL、认证方式（HTTP/SSH）
- 支持立即同步与后台同步

### 10.2 快捷键与 AI 默认项

- 主窗口快捷键（默认 `Alt + Space`）
- 快速 AI 会话条快捷键（默认 `Alt + Shift + A`）
- 默认 AI 工作目录
- 默认 AI 模型

### 10.3 外观与语言

- 主题：System / Dark / Light
- 语言：中文 / English

### 10.4 网络代理

- HTTP / HTTPS / SOCKS5
- 可选认证账号密码
- 支持连通性测试

### 10.5 安全

- 修改主密码
- 敏感配置本地加密存储

## 11. 常见问题

### Q1：终端提示找不到 `onespace` 命令

把 `~/.local/bin` 加入 `PATH`，例如：

```bash
export PATH="$HOME/.local/bin:$PATH"
```

### Q2：切换环境后 CLI 没生效

在 AI Environments 中确认：

1. 已 `Save`
2. 已 `Apply to CLI`
3. 对应工具 `Env Managed` 为开启（Claude/Codex/Gemini）

### Q3：macOS 提示 “OneSpace 已损坏”

参考 README 中的「macOS 安装与运行」章节执行 `xattr` 修复命令。
