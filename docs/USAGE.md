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
- 如果要使用 AI News，确认网络可以访问你在 `Settings -> News` 中添加的 RSS 源；设置页内置推荐 `36Kr` 和 `开源中国`
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

- 服务商列表中的“复制创建”只以 OneSpace 已保存的 canonical 配置生成未保存草稿；草稿使用全新的身份，名称可编辑，必须显式保存后才会创建记录
- 复制会递归移除 API Key、token、secret、password、auth 等敏感字段，也不会继承激活、收藏或历史状态；复制过程不会读取或合并本机 OpenCode 运行时配置
- 模型快捷表单可动态增删模型，并编辑模型 ID、名称、可选 cost、可选 limit、options 和 variants
- cost 表示每 100 万 token 的计费数值；OpenCode 配置未声明币种，因此 OneSpace 不推断或写入币种
- options 下拉只提供常见字段建议，并非完整字段目录；自定义键可使用 string、number、boolean 或合法 JSON 值
- 支持直接编辑高级 provider JSON；有效 JSON 与模型快捷表单实时双向同步，provider、模型及嵌套结构中的未知合法字段会保留
- JSON 语法或结构无效时，模型快捷表单保留最后一次有效快照并冻结，Save 同时禁用；修复为有效 JSON 后立即恢复同步和保存
- 保存时会保留 JSON 历史版本
- 可以从历史版本回滚到旧内容，再重新保存

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

## 7. Workspaces

`Workspaces` 用来把项目目录、会话、MCP、Skills 和 Subagents 绑定到同一个工作区里。

### 7.1 适合什么时候使用

- 一个仓库长期使用同一组 AI 会话、MCP 和能力扩展
- 想按项目查看历史会话，而不是只按工具筛选
- 想把项目级 Skills / Subagents 放进仓库目录，方便团队协作
- 想为某个项目预先绑定可用的 MCP Server

### 7.2 工作区列表

列表页支持：

- 创建、编辑、删除工作区
- 选择项目根目录
- 设置描述与标签
- 按标签筛选
- 查看每个工作区的会话数量

### 7.3 工作区详情

进入某个工作区后，主要标签包括：

- `Sessions`
  说明：查看并恢复这个项目目录下的 AI 会话
- `MCP`
  说明：把 MCP Server 绑定到当前工作区，并选择适用模型
- `Skills`
  说明：查看或复制当前工具下可用的 Skills 到项目范围
- `Subagents`
  说明：查看或复制当前工具下可用的 Subagents 到项目范围

### 7.4 与 Workflow Presets 的关系

两者解决的问题不同：

- `Workspaces` 偏项目资产管理，适合长期维护一个项目的会话与能力绑定
- `Workflow Presets` 偏启动模板，适合一键组合工具、目录、环境、MCP、Skills 和启动提示词

## 8. AI Workspace

`AI Workspace` 是应用内 AI 对话工作区，和 `AI Sessions` 的原生终端会话不同。

### 8.1 当前定位

- 在应用内创建和继续 AI 对话
- 管理可复用助手预设
- 配置 Quick Assistant 偏好
- 对接已配置的 AI provider 与模型目录

### 8.2 与 AI Sessions 的区别

- `AI Workspace`
  说明：应用内聊天体验，消息流会保存在 OneSpace 内，适合轻量问答、整理、改写和快速任务
- `AI Sessions`
  说明：在原生终端中启动 Claude / Codex / Gemini / OpenCode，适合编码、仓库操作和 CLI 原生能力

### 8.3 Quick Assistant

Quick Assistant 用于快速发起一段应用内对话：

- 可以从快捷窗口输入问题
- 会创建真实对话记录
- 后续可以回到 `AI Workspace` 中继续

## 9. AI Usage Stats

`AI Usage Stats` 从本地会话历史中统计 token 用量，不会请求云端账单接口。

### 9.1 统计范围

当前页面按工具分别展示：

- Claude
- Codex
- Gemini
- OpenCode

### 9.2 时间窗口

页面提供几个固定时间窗口，例如：

- 7 天
- 30 天
- 90 天

点击刷新按钮会重新扫描对应窗口内的本地记录。

### 9.3 数据含义

页面会展示：

- 扫描到的 sessions / calls
- total tokens
- calls
- sessions
- cache hit
- 每日趋势

注意：

- 如果对应工具没有可解析历史，页面会显示空状态
- 统计结果取决于本机 CLI 历史是否存在，以及 OneSpace 当前支持的解析格式

## 10. AI Flow

`AI Flow` 是面向计划驱动开发流程的辅助入口，用来发现和操作项目里的 `.ai-flow` 目录。

### 10.1 安装与健康检查

页面提供：

- 安装 AI Flow runtime
- 检查本地依赖
- 查看运行时健康状态

### 10.2 工作目录

可以添加包含 `.ai-flow` 目录的项目文件夹。

添加后，项目卡片会展示：

- AI Flow 项目状态
- 计划文件与状态文件入口
- 打开 AI Flow 目录
- 打开状态目录

### 10.3 会话与队列

当前页面还支持：

- 为某个计划启动 AI Flow 会话
- 创建队列
- 查看计划状态分类
- 对项目执行刷新

说明：

- AI Flow 依赖项目目录中的 `.ai-flow` 结构
- 如果项目没有对应目录，应先按 AI Flow 规范初始化

## 11. Skills 与 Subagents

详细说明见：[`docs/SKILLS.md`](./SKILLS.md)

这里只先给使用层面的总览。

### 11.1 三种视图

`Skills` 和 `Subagents` 都有以下结构：

- `Recommended`
  说明：来自源仓库同步的推荐项
- `Repository`
  说明：本地仓库镜像视图，包含远端同步结果与本地导入内容
- `Installed`
  说明：当前模型下已安装项目

### 11.2 安装范围

两者都支持两种安装范围：

- `Global`
  说明：面向当前工具的全局安装
- `Project`
  说明：安装到某个项目目录，只在该项目上下文中使用

### 11.3 Project Scope 的实际目录

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

### 11.4 Source 相关设置

在设置页里，`Skills 源` 和 `Subagents 源` 都支持：

- 添加 Git 源
- 启用/禁用源
- 设置默认适用模型
- 配置自动同步开关与间隔
- 导入/导出源 JSON

### 11.5 Subagents 的额外能力

`Subagents` 相比 `Skills` 多了一个源诊断能力，可用于检查：

- frontmatter 缺失
- `name` 缺失
- `name` 非法
- 文件读取失败

## 12. MCP Servers

详细说明见：[`docs/MCP.md`](./MCP.md)

日常使用建议：

1. 先用模板或手动方式创建 MCP
2. 视需要把它链接到某个环境
3. 再为具体工具启用模型开关
4. 如果状态看起来不一致，使用“刷新本地安装状态”

## 13. SSH Servers 与 SSH Tunnels

OneSpace 里有两个 SSH 相关入口，职责不同。

### 13.1 SSH Servers

`SSH Servers` 页面分为几个视图：

- `config`
- `history`
- `ignored`
- `custom`

当前实现支持：

- 读取 `~/.ssh/config`
- 收藏与忽略主机
- 保存最近连接历史
- 自定义连接
- 自定义连接支持密码模式和私钥文件模式

说明：

- `SSH Servers` 当前主要用于 macOS 原生终端 SSH 会话
- Windows 上应优先使用 `SSH Tunnels`

### 13.2 SSH Tunnels

`SSH Tunnels` 用来维护端口转发配置，而不是直接打开远程 shell。

支持的转发模式：

- `Local`
  说明：把本机端口转发到远端服务
- `Remote`
  说明：让 SSH 服务器上的端口转发回本机服务
- `Dynamic`
  说明：创建本地 SOCKS5 代理

页面支持：

- 使用已保存的 SSH Server
- 使用自定义 SSH 主机
- 测试连接
- 手动连接 / 断开
- 自动连接
- 断线、网络恢复或系统唤醒后自动重连
- 环境分组过滤

## 14. Protocol Router

`Protocol Router` 是本地协议路由工具，用来给 AI provider 暴露可复用的本地 endpoint。

### 14.1 使用场景

- Claude profile 需要走本地 Anthropic-compatible route
- OpenAI-compatible provider 需要统一配置本地转发入口
- 想在 OneSpace 里查看 route 状态、连接测试和近期请求用量

### 14.2 设置入口

基础运行配置在 `Settings -> Protocol Router` 中维护，包括：

- 是否启用
- 本地端口
- 请求记录保留天数
- Router token

完整的 route 状态、测试、复制 endpoint 与请求统计在 `Protocol Router` 工作区里查看。

### 14.3 与 AI Environments 的关系

在 `AI Environments` 中选择 `Protocol Router（协议路由）` 模式时，OneSpace 会根据 provider 生成或使用本地 route。

注意：

- Router token 轮换后，已有客户端需要使用新 token
- 如果 route 状态异常，先在 Protocol Router 工作区执行连接测试

## 15. Launcher 与 More Tools

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

### 15.1 More Tools

侧边栏里的 `More Tools` 是一组低频但重要的工具入口。

当前主要包含：

- `SSH Servers`
- `SSH Tunnels`
- `Protocol Router`
- `File Sharing`
- `Bookmarks`
- `Cloud Drive`
- `Documentation`

这些入口也会参与 Launcher 的内部跳转能力。

### 15.2 File Sharing

`File Sharing` 用于在可信局域网内临时提供本地文件下载。它默认出现在 `More Tools` 和 `Launcher` 的内部工具中。

发送文件的步骤：

1. 进入 `More Tools -> File Sharing`，或从 `Launcher` 打开 `File Sharing`
2. 通过文件选择器选择一个或多个普通文件，可继续添加、移除或清空选择
3. 选择要绑定的私有 IPv4 网卡地址；列表只显示检测到的可信局域网地址
4. 点击 `Start sharing`
5. 将页面显示的临时 HTTP 链接复制给接收方，或让接收方扫描二维码
6. 接收方在文件列表页逐个下载文件；发送方可在页面查看文件列表、传输记录和累计统计

使用边界：

- 仅支持可信局域网内的私有 IPv4 地址和 HTTP 下载，不支持公网、IPv6、TLS 或自动恢复
- 接收方只能下载，不能上传、浏览目录、打包 ZIP、在线预览或修改发送方文件
- 同一链接在共享停止前可以被多个设备重复使用；取得完整链接的任何人都能下载本次共享的全部文件
- HTTP 不提供传输加密，不能防止同一网络中的被动监听；不要在不可信网络中分享敏感文件
- 共享状态、令牌、文件列表和传输记录只保存在当前 OneSpace 进程内，不写入 Cloud Drive、同步、备份、数据库或配置持久化

停止行为：

- 点击 `Stop sharing` 会使链接和二维码立即失效；有进行中的下载时，停止操作会先确认并中断这些下载
- 停止后页面保留本次文件摘要和最终传输统计，但不再显示可用链接或二维码；可以重新选择文件并启动新会话
- 切换 OneSpace 页面或关闭主窗口只会隐藏窗口，共享仍会继续
- 通过托盘退出或真正退出 OneSpace 时，共享服务、令牌和正在进行的下载都会停止，旧链接立即不可访问

## 16. OmniSearch

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

## 17. Snippets、Bookmarks、Notes

这些模块属于本地内容型工具：

- `Snippets`
  说明：代码片段管理，支持语言、标签、复制
- `Bookmarks`
  说明：保存网址、路径或本地项目等常用入口，支持搜索、标签、收藏状态和打开目标
- `Notes`
  说明：Markdown 风格笔记

它们都会参与：

- 侧边栏计数
- OmniSearch 聚合搜索
- 同步策略中的 `content`

## 18. AI News

`AI News` 是一个真正可用的资讯模块，从用户已配置的 RSS 源抓取新闻并在本地按关键词过滤。

### 18.1 数据源

`Settings -> News` 内置以下推荐 RSS 源，可手动添加：

- `36Kr`：`https://www.36kr.com/feed`
- `开源中国`：`https://www.oschina.net/news/rss`

这些推荐源不会自动写入用户配置；删除后也不会被自动补回。

### 18.2 可配置项

在 `Settings -> News` 中可以设置：

- 是否启用自动同步
- 自动同步间隔
- 保留天数
- 最大保留条数
- 关键词（逗号、分号或换行分隔；标题、摘要或来源命中任一关键词即保留）
- RSS 源列表，可新增、编辑、删除，也可单独启用或禁用
- 内置推荐列表，可一键加入设置草稿，保存后生效

### 18.3 页面行为

- 列表按发布时间倒序
- 支持手动刷新
- 支持直接打开原文
- 会标记新内容
- 会提示 RSS 源访问或网络错误

## 19. Mail

`Mail` 当前是 Gmail 集成，不是通用 IMAP 客户端。

### 19.1 连接方式

- 需要你自己提供 Google OAuth Client ID / Client Secret
- 通过应用内 OAuth 流程完成授权
- 权限范围使用 Gmail 修改权限

### 19.2 当前支持的能力

- 收件箱列表
- 未读状态
- 邮件详情查看
- HTML / Text 正文解析
- 附件列表与下载
- 发信 / 快速回复
- 侧边栏未读数刷新

## 20. Cloud Drive

这一块请务必按当前实现理解。

### 20.1 当前已经完成的部分

- 保存阿里云盘 Refresh Token
- 连接态切换
- 文件浏览器界面
- 面包屑导航
- 示例目录/文件列表

### 20.2 当前还不应视为正式完工的部分

- 实际云端 API 集成仍是模拟流程
- 上传/下载按钮目前不应视为完整可用
- 文档、图片等真实预览能力尚未完成

换句话说：

- 它目前更像实验性占位模块，而不是生产可用的云盘客户端

## 21. Fish Pond

`Fish Pond` 是放松模块，入口在主界面底部鱼形图标。

当前内置：

- `CyberMuyu`
- `Snake`
- `Tetris`
- `Sudoku`
- `Minesweeper`
- `Wordle`

## 22. Settings

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

### 22.1 Data Storage

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

### 22.2 News

可配置：

- AI News 自动同步开关
- 同步间隔
- 保留策略
- 关键词
- RSS 源列表，可配置多个源并支持编辑、删除、启用和禁用
- 内置推荐列表，可将 `36Kr`、`开源中国` 加入当前设置草稿

### 22.3 General

目前主要是：

- `Launch at Login`

### 22.4 Updates

可配置：

- 自动更新开关
- 检查更新间隔

### 22.5 Skills 源 / Subagents 源

可配置：

- 自动同步开关
- 自动同步间隔
- “新内容”徽标的持续小时数
- Git 源清单
- 导入 / 导出 JSON
- 手动 `Sync Now`

### 22.6 Network Proxy

支持：

- `http`
- `https`
- `socks5`
- 用户名 / 密码
- 连通性测试
- 周期性可用性检查

### 22.7 Shortcuts

当前可录制和保存两个全局快捷键：

- 主窗口显示/隐藏
- Quick AI Session 浮动条

默认值：

- 主窗口：`Alt+Space`
- Quick AI：`Alt+Shift+A`

### 22.8 AI Terminal

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

### 22.9 Appearance

支持：

- 语言切换
- 主题切换

### 22.10 Security

支持：

- 查看当前主密码
- 修改主密码
- 自动生成随机密码

## 23. 托盘与窗口行为

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

## 24. CLI

命令行说明见：[`docs/CLI.md`](./CLI.md)

建议理解为：

- `onespace ai ...` 用于从终端快速创建会话
- `onespace resume ...` 用于从任意终端统一恢复已保存会话
- `onespace env ...` 用于查看或切换 OneSpace 记录的活动环境绑定

## 25. 常见问题

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

### Q7：Cloud Drive 为什么看起来像“半成品”

因为当前实现确实仍是实验性/模拟阶段：

- UI 已经搭好
- 真实云盘能力还没全部接入

### Q8：macOS 提示 “OneSpace 已损坏”

执行：

```bash
sudo xattr -cr /Applications/OneSpace.app
```
