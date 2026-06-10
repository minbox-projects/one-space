# OneSpace 使用手册

这份手册按“初始化 -> 核心 AI 流程 -> 周边工具 -> 设置与同步”的顺序整理，重点是把当前代码里已经落地的行为、限制和推荐操作路径说清楚。

## 1. 适用范围与使用前提

- 当前产品是 `macOS-first` 桌面应用
- AI 会话、应用启动、SSH 连接等能力依赖 macOS 原生终端与 `open`/AppleScript
- AI 能力围绕 4 个 CLI 展开：`Claude`、`Codex`、`Gemini`、`OpenCode`
- 云同步采用 `local-first` 思路：运行时先读写本地镜像，再按配置同步到 `local / iCloud / Git`

开始前建议准备：

- 至少安装一种目标 AI CLI
- 准备对应 API Key 或本机已存在的 CLI 配置
- 如果要使用 AI News，确认网络可以访问默认 RSS 源 `36Kr` 和 `开源中国`
- 如果要使用 Gmail，准备 Google OAuth Client ID / Client Secret

## 2. 首次启动

首次启动会进入初始化向导，只做两件事：

1. 选择数据存储方式
2. 设置主密码

### 2.1 数据存储方式

- `local`
  说明：数据保存在本机，最简单，适合单机使用
- `icloud`
  说明：数据镜像位于 iCloud Drive 下，适合 Apple 生态多设备同步
  限制：路径必须位于 `com~apple~CloudDocs` 之下
- `git`
  说明：使用 Git 仓库同步数据
  说明：初始化向导阶段只选择模式，仓库细节建议后续在设置页补充

### 2.2 主密码

- 主密码用于保护本地敏感信息
- 向导默认会生成一串随机值，便于先完成初始化
- 后续可在 `Settings -> Security` 里修改

## 3. 主界面导航总览

侧边栏主入口包括：

- `Launcher`
- `AI Sessions`
- `AI Environments`
- `AI News`
- `Skills`
- `Subagents`
- `MCP Servers`
- `SSH`
- `Snippets`
- `Bookmarks`
- `Notes`
- `Mail`
- `Documentation`
- `Settings`

底部额外入口包括：

- `Fish Pond`
- 主题切换
- 语言切换
- GitHub 仓库
- 关于窗口

## 4. AI Environments

`AI Environments` 是 OneSpace 的核心页面，用来统一管理不同 AI CLI 的环境预设与配置投影。

### 4.1 支持的工具

- `Claude`
- `Codex`
- `Gemini`
- `OpenCode`

### 4.2 页面会做什么

页面打开后会自动执行几类检查：

- 检测本机 CLI 是否安装，并显示版本
- 检测系统是否存在对应 CLI 配置
- 对 `Claude`、`Codex`、`Gemini` 尝试自动导入系统默认配置
- 读取其它已同步设备上的环境，允许一键导入到当前机器

### 4.3 系统配置自动导入

自动导入主要读取这些位置：

- `Claude`：`~/.claude/settings.json`
- `Codex`：`~/.codex/auth.json`、`~/.codex/config.toml`
- `Gemini`：`~/.gemini/.env`、`~/.gemini/settings.json`
- `OpenCode`：`~/.config/opencode/opencode.json`

说明：

- 目前自动导入的重点是 `Claude`、`Codex`、`Gemini`
- `OpenCode` 以读取现有 provider 配置为主，不走相同的 “Env Managed” 流程
- 如果导入到的配置缺少 `API Key` 或 `Base URL`，环境可能会导入成功但不会自动激活
- `Claude` 导入和保存会同步维护 `~/.claude/settings.json` 顶层 `model`、`env.ANTHROPIC_MODEL` 以及 OneSpace 内部 `claude_default_model`
- 如果 `Claude` 的顶层 `model` 与 `env.ANTHROPIC_MODEL` 不一致，OneSpace 导入时以 `env.ANTHROPIC_MODEL` 为准
- 如果清空 `Claude` 默认模型，OneSpace 会同时移除顶层 `model` 和 `env.ANTHROPIC_MODEL`，不会保留空字符串

### 4.4 环境预设的核心概念

- `Provider / Preset`
  说明：一组与某个工具绑定的配置记录
- `Save`
  说明：把当前编辑内容保存到 OneSpace 数据中
- `Apply to CLI`
  说明：把当前环境设为活动环境，并把配置写入目标 CLI 配置文件
- `Env Managed`
  说明：仅对 `Claude`、`Codex`、`Gemini` 生效，决定后续 CLI 配置是否继续由 OneSpace 接管

### 4.5 各工具可配置项

#### Claude

支持的常见字段包括：

- API Key / Base URL
- 默认模型
- `reasoning / haiku / sonnet / opus` 路由模型
- `dangerously_skip_permissions`
- `enable_all_memory_features`
- `enable_mcp`
- `allowed_tools`
- `blocked_tools`
- `max_session_turns`

补充：

- 页面里还提供 “跳过 Claude 引导登录” 的辅助操作，偏排障用途

#### Codex

支持的常见字段包括：

- API Key / Base URL / Model
- `disable_response_storage`
- `personality`
- `wire_api`
- `model_reasoning_effort`
- `model_reasoning_summary`
- `approval_policy`
- `sandbox_mode`

#### Gemini

支持的常见字段包括：

- API Key / Base URL / Model
- `gemini_auth_type`
- `theme`
- `vim_mode`
- `default_approval_mode`

#### OpenCode

OpenCode 与前三者不同：

- 支持直接编辑 provider JSON
- 保存时会保留 JSON 历史版本
- 可以从历史版本回滚到旧内容，再重新保存
- 还支持额外设置：
  - `opencode_default_model`
  - `opencode_default_agent`
  - `opencode_sessions_dir`
  - `small_model`
  - `timeout`
  - `share_mode`

### 4.6 环境激活的推荐顺序

1. 先保存环境
2. 再执行 `Apply to CLI`
3. 对 `Claude`、`Codex`、`Gemini` 确认 `Env Managed` 状态符合预期

如果只保存不应用：

- 环境会存在于 OneSpace 内
- 但 CLI 配置文件不一定被立即改写

### 4.7 导入导出与多机协作

页面支持：

- 导出所有环境为 JSON
- 导入环境文件，并在导入前预览冲突
- 冲突项可按条选择 `overwrite` 或 `new`
- 从其它同步设备读取环境，并一键复制成当前设备上的新环境后激活

## 5. AI Sessions

`AI Sessions` 用来创建、恢复和整理原生终端会话。

### 5.1 当前实际的创建流程

点击 `New Session` 后，当前实现的创建入口更接近“选择工具/工作流 + 目录”，而不是传统的“先输入会话名”：

1. 选择工具，或直接选择某个 `Workflow Preset`
2. 选择工作目录
3. 点击创建
4. OneSpace 在原生终端里启动对应 CLI

重要说明：

- 手动创建的新会话默认不会先让你输入名字
- 会话名会在后续从 CLI 历史里自动回填
- 如果还没回填，会看到类似“正在从历史同步标题”的占位状态
- 你也可以之后手动重命名

### 5.2 会话的真实来源

OneSpace 的会话列表来自两部分：

- 本地记录的会话元数据
- 定时从各 CLI 历史中回填的标题、模型、原生会话 ID

因此会出现两种情况：

- 你刚创建时，列表里先有占位记录
- 稍后会自动补齐 `tool_session_id`、标题和模型名

### 5.3 会话支持的操作

- 恢复会话
- 重命名会话
- 删除会话
- 复制会话 ID
- 按工具筛选
- 按模型筛选
- 按名称搜索

### 5.4 CLI 安装按钮

页面右上角的 `Install CLI` 会把 `onespace` 脚本安装到：

```bash
~/.local/bin/onespace
```

### 5.5 Quick AI Session Bar

默认快捷键：

```text
Alt + Shift + A
```

Quick Bar 当前支持：

- 选择默认工具
- 直接启动会话
- 选择工作目录
- 选择 `Workflow Preset`
- 自动读取 `Settings -> AI Terminal` 中配置的默认目录与默认模型

补充：

- `Enter` 会立即启动
- `Esc` 会关闭浮动条

## 6. Workflow Presets

工作流预设用于把“目录 + 工具 + 环境 + MCP + Skills + 启动提示词”打包成一份可复用模板。

### 6.1 可配置项

- 名称
- 目标工具
- 默认工作目录
- 目标环境 `provider_id`
- MCP Server 列表
- Required Skills 列表
- Launch Prompt
- Launch Scope

### 6.2 Launch Scope

- `Shared`
  说明：偏全局模式，依赖会尽量应用到该工具的共享环境
- `Strict`
  说明：偏隔离模式，运行时会尽量走会话隔离配置

### 6.3 依赖检查

OneSpace 会检查：

- 缺失的 MCP Server
- 已存在但未为当前工具启用的 MCP Server
- 缺失的 Skills
- 可自动安装的 Skills

### 6.4 一键补依赖

如果工作流缺依赖，可以直接执行 `Apply Dependencies`：

- 自动建立 MCP 链接
- 自动启用对应工具的 MCP 开关
- 自动安装能确定来源的 Skills

### 6.5 最近运行记录

`Workflow` 标签页会记录运行历史，支持：

- 查看成功率
- 按预设筛选
- 重放某次运行
- 重新恢复对应会话
- 手动标记成功/失败
- 复制当次运行使用的启动提示词
- 删除运行记录

## 7. Skills 与 Subagents

详细说明见：[`docs/SKILLS.md`](./SKILLS.md)

这里只先给使用层面的总览。

### 7.1 三种视图

`Skills` 和 `Subagents` 都有以下结构：

- `Recommended`
  说明：来自源仓库同步的推荐项
- `Repository`
  说明：本地仓库镜像视图，包含远端同步结果与本地导入内容
- `Installed`
  说明：当前模型下已安装项目

### 7.2 安装范围

两者都支持两种安装范围：

- `Global`
  说明：面向当前工具的全局安装
- `Project`
  说明：安装到某个项目目录，只在该项目上下文中使用

### 7.3 Project Scope 的实际目录

Skills 的项目目录：

- Claude：`<project>/.claude/skills`
- Codex：`<project>/.agents/skills`
- Codex 兼容目录：`<project>/.codex/skills`
- Gemini：`<project>/.gemini/skills`
- OpenCode：`<project>/.opencode/skills`

Subagents 的项目目录：

- Claude：`<project>/.claude/agents`
- Codex：`<project>/.codex/agents`
- Gemini：`<project>/.gemini/agents`
- OpenCode：`<project>/.opencode/agents`

### 7.4 Source 相关设置

在设置页里，`Skills 源` 和 `Subagents 源` 都支持：

- 添加 Git 源
- 启用/禁用源
- 设置默认适用模型
- 配置自动同步开关与间隔
- 导入/导出源 JSON

### 7.5 Subagents 的额外能力

`Subagents` 相比 `Skills` 多了一个源诊断能力，可用于检查：

- frontmatter 缺失
- `name` 缺失
- `name` 非法
- 文件读取失败

## 8. MCP Servers

详细说明见：[`docs/MCP.md`](./docs/MCP.md)

日常使用建议：

1. 先用模板或手动方式创建 MCP
2. 视需要把它链接到某个环境
3. 再为具体工具启用模型开关
4. 如果状态看起来不一致，使用“刷新本地安装状态”

## 9. Launcher

`Launcher` 不只是应用启动器，还可以当作轻量的命令中心。

支持的类型：

- `app`
- `script`
- `url`
- `folder`
- `internal`

其中：

- `internal` 用于跳转到 OneSpace 内部页面
- `script` 有信任开关，未信任脚本执行前会二次确认

页面还支持：

- 搜索
- Pin / Unpin
- 调整置顶顺序
- 导入 / 导出 JSON

## 10. OmniSearch

快捷键：

```text
Cmd/Ctrl + K
```

会聚合搜索以下内容：

- 会话
- Launcher 项
- SSH 主机与历史
- Snippets
- Bookmarks
- Notes
- Skills
- 工作流预设与运行记录

## 11. SSH

`SSH` 页面分为几个视图：

- `config`
- `history`
- `ignored`
- `custom`

当前实现支持：

- 读取 `~/.ssh/config`
- 收藏与忽略主机
- 保存最近连接历史
- 自定义连接
- 自定义连接支持：
  - 密码模式
  - 私钥文件模式

## 12. Snippets、Bookmarks、Notes

这些模块属于本地内容型工具：

- `Snippets`
  说明：代码片段管理，支持语言、标签、复制
- `Bookmarks`
  说明：保存网址、路径等常用入口
- `Notes`
  说明：Markdown 风格笔记

它们都会参与：

- 侧边栏计数
- OmniSearch 聚合搜索
- 同步策略中的 `content`

## 13. AI News

`AI News` 是一个真正可用的资讯模块，默认从 RSS 源抓取新闻并在本地按关键词过滤。

### 13.1 数据源

当前会自动补齐以下 RSS 源：

- `36Kr`：`https://www.36kr.com/feed`
- `开源中国`：`https://www.oschina.net/news/rss`

### 13.2 可配置项

在 `Settings -> News` 中可以设置：

- 是否启用自动同步
- 自动同步间隔
- 保留天数
- 最大保留条数
- 关键词（逗号、分号或换行分隔；标题、摘要或来源命中任一关键词即保留）
- RSS 源列表，可新增、编辑、删除，也可单独启用或禁用

### 13.3 页面行为

- 列表按发布时间倒序
- 支持手动刷新
- 支持直接打开原文
- 会标记新内容
- 会提示 RSS 源访问或网络错误

## 14. Mail

`Mail` 当前是 Gmail 集成，不是通用 IMAP 客户端。

### 14.1 连接方式

- 需要你自己提供 Google OAuth Client ID / Client Secret
- 通过应用内 OAuth 流程完成授权
- 权限范围使用 Gmail 修改权限

### 14.2 当前支持的能力

- 收件箱列表
- 未读状态
- 邮件详情查看
- HTML / Text 正文解析
- 附件列表与下载
- 发信 / 快速回复
- 侧边栏未读数刷新

## 15. Cloud Drive

这一块请务必按当前实现理解。

### 15.1 当前已经完成的部分

- 保存阿里云盘 Refresh Token
- 连接态切换
- 文件浏览器界面
- 面包屑导航
- 示例目录/文件列表

### 15.2 当前还不应视为正式完工的部分

- 实际云端 API 集成仍是模拟流程
- 上传/下载按钮目前不应视为完整可用
- 文档、图片等真实预览能力尚未完成

换句话说：

- 它目前更像实验性占位模块，而不是生产可用的云盘客户端

## 16. Fish Pond

`Fish Pond` 是放松模块，入口在主界面底部鱼形图标。

当前内置：

- `CyberMuyu`
- `Snake`
- `Tetris`
- `Sudoku`
- `Minesweeper`
- `Wordle`

## 17. Settings

设置页是按标签分区保存的，当前标签包括：

- `Data Storage`
- `News`
- `General`
- `Updates`
- `Skills 源`
- `Subagents 源`
- `Network Proxy`
- `Shortcuts`
- `AI Terminal`
- `Appearance`
- `Security`

### 17.1 Data Storage

这里可以配置：

- 存储类型：`local / icloud / git`
- Git 地址与认证方式
- iCloud 路径
- 本地数据路径
- 同步策略

当前可选同步策略项包括：

- `providers`
- `mcp`
- `content`
- `workflow_presets`
- `skills_sources`
- `skills_repository`
- `subagents_sources`
- `subagents_repository`
- `ai_news`

### 17.2 News

可配置：

- AI News 自动同步开关
- 同步间隔
- 保留策略
- 关键词
- RSS 源列表，可配置多个源并支持编辑、删除、启用和禁用

### 17.3 General

目前主要是：

- `Launch at Login`

### 17.4 Updates

可配置：

- 自动更新开关
- 检查更新间隔

### 17.5 Skills 源 / Subagents 源

可配置：

- 自动同步开关
- 自动同步间隔
- “新内容”徽标的持续小时数
- Git 源清单
- 导入 / 导出 JSON
- 手动 `Sync Now`

### 17.6 Network Proxy

支持：

- `http`
- `https`
- `socks5`
- 用户名 / 密码
- 连通性测试
- 周期性可用性检查

### 17.7 Shortcuts

当前可录制和保存两个全局快捷键：

- 主窗口显示/隐藏
- Quick AI Session 浮动条

默认值：

- 主窗口：`Alt+Space`
- Quick AI：`Alt+Shift+A`

### 17.8 AI Terminal

这是非常重要的一页，用来控制会话创建体验。

可配置：

- 默认工作目录
- 默认 AI 模型
- 终端应用名称
- 各模型的创建命令模板

默认启动命令：

- Claude：`claude --session-id {session_id}`
- Gemini：`gemini`
- Codex：`codex`
- OpenCode：`opencode`

### 17.9 Appearance

支持：

- 语言切换
- 主题切换

### 17.10 Security

支持：

- 查看当前主密码
- 修改主密码
- 自动生成随机密码

## 18. 托盘与窗口行为

OneSpace 默认是“更接近常驻工具”的窗口行为：

- 主窗口关闭时通常会隐藏而不是彻底退出
- 托盘菜单可快速打开：
  - 主窗口
  - Quick AI Session
  - 全局搜索
  - Launcher
  - AI Sessions
  - AI Environments
  - Notes
  - Snippets
  - Settings
  - Sync Now

## 19. CLI

命令行说明见：[`docs/CLI.md`](./CLI.md)

建议理解为：

- `onespace ai ...` 用于从终端快速创建会话
- `onespace resume ...` 用于从任意终端统一恢复已保存会话
- `onespace env ...` 用于查看或切换 OneSpace 记录的活动环境绑定

## 20. 常见问题

### Q1：终端提示找不到 `onespace`

把 `~/.local/bin` 加到 `PATH`：

```bash
export PATH="$HOME/.local/bin:$PATH"
```

### Q2：环境已经切换，但 CLI 里看起来没变

优先检查：

1. 是否在 `AI Environments` 中执行过 `Save`
2. 是否执行过 `Apply to CLI`
3. `Claude` / `Codex` / `Gemini` 的 `Env Managed` 是否开启

### Q3：为什么 `onespace resume <session_id>` 可以恢复不同工具的会话

因为 `onespace resume` 是统一入口。

它会先从 OneSpace 当前的会话状态里找到这条会话，进入保存时的工作目录，再按工具转成各自的原生命令，例如：

- Claude -> `claude -r`
- Gemini -> `gemini -r`
- Codex -> `codex resume`
- OpenCode -> `opencode -s`

### Q4：新建会话时为什么没有名字输入框

这是当前实现行为：

- 手动创建时先创建会话记录并启动 CLI
- 会话标题会在稍后从 CLI 历史中自动回填
- 你也可以事后手动改名

### Q5：为什么 Skills/Subagents 要区分 Global 和 Project

因为当前实现支持两种投放方式：

- 全局安装适合通用能力
- 项目安装适合仓库内私有能力、隔离依赖和团队协作

### Q6：AI News 没内容

通常按这个顺序排查：

1. `Settings -> News` 是否启用自动同步
2. 当前网络是否能访问 `https://www.36kr.com/feed` 和 `https://www.oschina.net/news/rss`
3. 关键词是否过窄，导致 RSS 条目被本地过滤
4. RSS 源是否临时不可用或返回错误状态

### Q6：Cloud Drive 为什么看起来像“半成品”

因为当前实现确实仍是实验性/模拟阶段：

- UI 已经搭好
- 真实云盘能力还没全部接入

### Q7：macOS 提示 “OneSpace 已损坏”

执行：

```bash
sudo xattr -cr /Applications/OneSpace.app
```
