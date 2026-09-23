# OneSpace 使用手册

这份手册按“初始化 -> 核心 AI 流程 -> 周边工具 -> 设置与同步”的顺序整理，重点是把当前代码里已经落地的行为、限制和推荐操作路径说清楚。

## 1. 适用范围与使用前提

- 当前产品是 `macOS-first` 桌面应用
- AI 会话、应用启动、SSH 连接等能力依赖 macOS 原生终端与 `open`/AppleScript
- AI 能力围绕 4 个 CLI 展开：`Claude`、`Codex`、`Antigravity`、`OpenCode`
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
- `Antigravity`
- `OpenCode`

### 4.2 页面会做什么

页面打开后会自动执行几类检查：

- 检测本机 CLI 是否安装，并显示版本
- 检测系统是否存在对应 CLI 配置
- 对 `Claude`、`Codex`、`Antigravity` 尝试自动导入系统默认配置
- 读取其它已同步设备上的环境，允许一键导入到当前机器

### 4.3 系统配置自动导入

自动导入主要读取这些位置：

- `Claude`：`~/.claude/settings.json`
- `Codex`：`~/.codex/auth.json`、`~/.codex/config.toml`
- `Antigravity`：`~/.gemini/antigravity-cli/settings.json`
- `OpenCode`：`~/.config/opencode/opencode.json`

说明：

- 目前自动导入的重点是 `Claude`、`Codex`、`Antigravity`
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
  说明：仅对 `Claude`、`Codex`、`Antigravity` 生效，决定后续 CLI 配置是否继续由 OneSpace 接管

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

#### Antigravity

Antigravity CLI（二进制 `agy`）由 OneSpace 按环境托管：应用配置时会写入或合并 `~/.gemini/antigravity-cli/settings.json` 中的 `modelProvider` 与模型，并在启动 `agy` 时注入 `GEMINI_API_KEY` 与 `GOOGLE_GEMINI_BASE_URL`；不会写入 `~/.gemini/.env` 或旧的 `~/.gemini/settings.json`。

支持的常见字段包括：

- API Key / Base URL / Model
- `antigravity_auth_type`（API Key / Google 账号）
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
3. 对 `Claude`、`Codex`、`Antigravity` 确认 `Env Managed` 状态符合预期

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
  说明：在原生终端中启动 Claude / Codex / Antigravity / OpenCode，适合编码、仓库操作和 CLI 原生能力

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
- Antigravity
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
- Antigravity 不在磁盘持久化 token 用量，其 token 列显示显式的“暂不可用”状态；OneSpace 不会解析 Antigravity 的会话记录来推算 token

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
- Antigravity：`<project>/.agents/skills/`
- OpenCode：`<project>/.opencode/skills`

Subagents 的项目目录：

- Claude：`<project>/.claude/agents`
- Codex：`<project>/.codex/agents`
- Antigravity：`<project>/.agents/agents/<name>/agent.md`
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

### 15.3 API 网关

`API 网关` 是把多个上游服务商聚合成一个本地入口的转发服务。它不是 `More Tools` 工具，而是左侧 `AI 能力` 分组下的独立功能 `API 网关`（位于 `AI 终端服务商` 与 `AI 用量统计` 之间），默认监听 `127.0.0.1:17688`（开发构建为 `127.0.0.1:17689`，两种构建共享同一份网关配置但端口按构建自动区分），并提供独立的上游服务商、本地 Key 与终端同步管理。

启用服务：

1. 打开左侧 `API 网关`，在 `本地服务` 卡片查看运行状态、端口与本地 Api 地址
2. 点击 `启动服务` / `停止服务` 切换监听；`本地 Api 地址`（形如 `http://127.0.0.1:<端口>/v1`，带 `/v1` 后缀）可直接复制给调用方
3. 启用状态会被保存，重启 OneSpace 后按上次状态自动恢复监听

使用边界：

- 只监听 `127.0.0.1` 与配置端口；端口被占用时启动失败并给出包含端口与原因的错误，不会自动改端口或回退到其他端口
- `本地服务` 卡片同时显示上游服务商数、自动禁用数与启用的本地 Key 数

维护上游与本地 Key：

- 每个上游维护名称、`Api 地址`、`ApiKey`、默认模型、模型映射与 `接口协议`；每条「本地模型 → 远端模型」映射都可选填 `本地模型名称`（网关中为该本地模型展示的名称，留空时回退到远端模型名），并可单独选择协议，默认「跟随服务商协议」（保存为继承而非固定值），因此在同一条上游记录内就能让不同模型走不同 endpoint
- 模型解析先按请求模型匹配映射：命中映射但该映射的协议与请求不一致时，该上游不参与本次请求，也不会回退默认模型；没有匹配到映射时才回退默认模型，并要求服务商协议与请求一致，两者都不可用时该上游不参与本次请求
- `接口协议` 决定该上游接收哪种请求：`Chat Completions (/chat/completions)` 或 `Responses (/responses)`。本地服务同时接受 `/chat/completions` 与 `/responses`（也接受不带 `/v1` 的写法），但只会把请求交给映射行协议（缺省继承服务商协议）一致的上游；映射行协议不一致的上游不参与本次请求，且不会做请求体转换，请让调用方协议与该上游协议保持一致
- `Api 地址` 写成 `https://host` 或 `https://host/v1` 均可，本地服务会归一化路径并折叠重复的 `/v1`
- 可以用 `预览模型` 查看某个本地模型最终会解析成哪个远端模型与目标 endpoint
- 已知边界：`/v1/messages`（Anthropic messages 协议）暂不支持；只提供该协议的模型（例如 OpenCode Go 的 Qwen / MiniMax 系列）本轮不可用
- 可以单独启用/禁用某个上游；禁用只影响是否参与转发，不会删除其配置
- 新建本地 Key 只需填写名称，Key 值由应用随机生成
- 本地 Key 是调用方访问本地服务时使用的凭证，请求通过 `Authorization: Bearer <key>` 或 `x-api-key` 携带，任一启用 Key 均可用；没有任何启用 Key 时所有请求都会被拒绝
- 默认 Key 在 `本地 Key` 列表中标记；手动指定的默认 Key 被禁用或删除后，会按列表顺序顺延到下一个启用 Key
- 列表中的 Key 只显示掩码值，可复制完整值；密钥不会以明文落盘

把本地 Api / Key 一键配置到 OpenCode / Codex：

1. 打开 `AI 终端集成`；列表固定为每个受支持工具（`opencode`、`codex`）各一行，不依赖该工具是否已有服务商记录
2. 点目标工具所在行最右侧的单个按钮：尚未添加过显示 `添加服务商`（调用 `api_gateway_configure_terminal`），已添加过显示 `同步`（调用 `api_gateway_sync_terminal`）
3. 写入内容是本地 Api 地址、当前默认本地 Key，以及网关的模型映射列表

说明：

- 同步不会改写你已有的 `opencode` / `codex` 服务商记录，而是为每个工具创建或更新一条名为 `API Gateway` 的独立网关服务商记录
- 该记录不会自动启用；需要使用时到 `AI Environments` 中手动启用，不需要时可直接禁用或删除
- 写入的模型清单只取当前已启用且未被自动禁用的上游服务商：`opencode` 按每条映射的本地模型名列出模型，并在该映射配置了推理档位时把档位写成模型条目的 `reasoning: true` 与 `variants`（每个档位形如 `{"reasoningEffort": "<档位>"}`），未配置档位的模型只写名称；`codex` 使用第一条映射的本地模型名作为模型，没有可用映射时才回退该服务商的默认模型
- 同步按本地同步台账的 provider id 更新同一条记录，不会每次新建；仅当该记录仍是该工具名下带网关标记的网关服务商时才复用，若台账指向的是你的普通服务商记录，则视为过期记录并新建独立的网关服务商，绝不会改写它；在 `AI Environments` 中删除网关记录后，再点该行的 `同步` 会自动新建一条全新的 `API Gateway` 记录
- 不读写 `claude`、`antigravity` 记录，也不改写 `Protocol Router` 的 route 数据
- 没有启用本地 Key 时，配置与同步会被拒绝并提示先新增并启用一个本地 Key
- 如需撤销，在 `AI Environments` 中禁用或删除该网关服务商记录

同步时机：

- 每行显示 `已同步` 或 `待同步` 状态徽标；状态由后端按本地同步台账计算（`pending_sync`），默认 Key 或本地 Api 地址与上次写入的不一致时即为 `待同步`，尚未写入过或找不到受标记的网关记录时也显示 `待同步`
- 修改端口或切换默认 Key 后，之前配置过的目标会重新显示为 `待同步`，需要再点该行的 `同步`
- 没有顶部全局按钮、多选框与批量操作；每次写入都由用户主动点击该行的单个按钮触发

自动禁用与恢复：

- 上游返回 401 / 403 会立即把该服务商的对应模型映射标记为自动禁用；这类鉴权失败不计入连续失败次数，只能通过手动重新启用恢复
- 网络错误、非 JSON 响应体，以及 5xx 连续失败达到 3 次，也会自动禁用该模型映射；一次成功的请求会把该映射的连续失败计数清零
- 因连续失败被自动禁用的映射在 60 秒冷却后可以自动恢复：当下一次需要该模型的请求无法被任何健康候选服务时，网关会额外探测该映射一次；探测成功即恢复服务并清零运行状态，探测失败则重新开始 60 秒冷却，调用方收到与其他候选耗尽相同的错误
- 机器休眠唤醒（检测到系统恢复）后的 60 秒内，本次恢复期间发生的网络类传输失败不计入连续失败、也不会触发自动禁用；HTTP 状态失败（如 5xx、额度耗尽 429）与窗口结束后的失败仍按原有规则计数
- 429 / 404 只切换到其他候选，不计入失败；400 / 422 等其余 4xx 会把上游错误直接返回给调用方，不切换也不禁用
- 自动禁用只影响转发候选，不会改写你设置的启用意图；在服务商列表点击 `手动重新启用` 可清除自动禁用状态与失败计数
- 自动禁用、自动恢复与手动重新启用都会让打开的 API 网关页即时刷新，无需切换页签或重启；服务商详情弹窗打开时也会同步这些映射的运行时状态，且不会覆盖你尚未保存的编辑

全部不可用时的表现：

- 非流式请求：返回 `502` 与 `all_providers_unavailable` 错误体，消息中列出各候选的失败原因
- 流式请求：在写出首字节前允许切换到其他候选，写出首字节后只终止本次流，不会重试或重复输出
- 没有任何可用候选时：非流式返回上述 `502`，流式返回 `200` 的 SSE 错误事件并以 `data: [DONE]` 结束
- 协议用错时（例如用 `/chat/completions` 调用只走 `/responses` 的模型），返回的 `502` 消息会指出该模型应走的 endpoint；流式请求返回的是同一错误对象的 `200` SSE 事件
- 只有 `POST /v1/chat/completions` 与 `POST /v1/responses` 会被转发；`GET /v1/models` 只返回本地模型名并集，不请求上游；其他路径或方法返回 `404`

### 15.4 用量统计与请求日志

`API 网关` 页面在既有的 `上游服务商` / `Api Keys` / `AI 终端集成` 页签之外新增「用量统计」与「请求日志」两个页签，入口仍在同一个 `API 网关` 页面内（未新增导航项）。页签采用面板常驻挂载、仅切换可见性的方式，切走再切回时会保留各自的范围、分组与页码等选择。

`用量统计`：

- 默认范围 `今日`，可选 `今日`、`昨天`、`近 7 天`、`近 15 天`、`近 30 天`、`全部`；日边界固定为 UTC+8，`昨天` 为 `[昨天 00:00, 今天 00:00)` 的完整自然日，`近 N 天` 表示包含今日在内共 N 个自然日
- 右上角刷新图标按钮按当前范围立即重新取数，请求期间显示刷新中状态
- 三张卡片分别显示当前范围的 `Tokens`、`请求数` 与 `花费（$）`；非流式响应只从 2xx 响应体解析 usage，流式请求在首个转发字节之后失败时记为 `failure` 但保留已累积 usage，其 tokens 与固化金额照常计入，未捕获用量的失败四档 token 为 0、不产生金额贡献；合计只累加已定价请求，未定价请求数量以提示条单独说明，`未定价` 与 `—` 只适用于已到达上游模型、没有匹配价格行且会产生用量成本的请求（成功，或记录的 `total_tokens > 0`），零用量与未到达上游的失败按 0 成本处理、不计入未定价；取消的工具 transport 不产生业务日志，也不进入统计
- `用量分析` 表按本地模型列出请求数、输入、缓存（读）、缓存（写）、输出与花费，并在每个模型下按本范围内实际调用过的上游服务商列出同样口径的明细行；未被调用过的服务商不会出现
- 单日范围（`今日`、`昨天`）下方的时间分布按 UTC+8 小时展示，只显示有数据的小时（不超过 24 行）；多日范围与 `全部` 按自然日展示

`请求日志`：

- 默认范围同为 `今日`，提供与用量统计相同的六个快捷范围与刷新图标按钮
- 分组可在 `不分组`、`模型`、`Day（UTC+8）` 之间切换：不分组时列为时间、状态、模型、Tokens、花费（$），按时间倒序；分组后列为分组对象、请求数、错误数与最后请求时间，其中错误数只统计 `failure`
- 过滤按钮展开选择式条件面板，状态只提供 `success` 与 `failure`，并可与模型条件同时生效；应用后列表只显示匹配记录并回到第 1 页，无匹配记录时显示空状态
- 不分组列表每页 50 条，默认第 1 页；切换筛选、分组或时间范围后回到第 1 页，页码越界时自动收敛到有效页
- 分组契约：不分组以 `group_by: "none"` 表示（前端即发送该值），`null`、缺省或空串按不分组兼容处理，`"model"` 与 `"day"` 分别表示按模型、按 UTC+8 自然日分组，其他值返回可操作错误；不分组响应的 `group_by` 为 `null`，分组响应回显分组值
- 不分组响应还返回当前范围内去重后的非空本地模型列表 `models`（受范围与状态筛选约束，不受分页与模型筛选影响），用于填充过滤面板的模型选择，因此当前页未出现的模型也能被筛选
- 工具侧 TCP 连接关闭或响应无法交付属于 transport 生命周期，不是一个已完成的业务请求日志结果。此时该 inbound request 的整批缓冲日志都会丢弃，包括断开前已完成的上游 attempts，也不会补写合成 `cancelled` 终止行；正常完成的 success、failure、重试后恢复与无候选请求仍按既有规则记录
- 旧版本已经写入的 `cancelled` 行不会被删除或迁移，但不会出现在任何用户可见结果中：列表、总数、页数、模型筛选项、按模型/按天分组、请求数、错误数和最后请求时间都会排除它们，界面也会防御性忽略 stale payload 中的 cancelled 记录。旧调用方仍可传 `status="cancelled"`，结果是合法空页而不是参数错误；兼容 enum/type 与状态翻译仍保留

`模型价格`维护：

- 入口在 `上游服务商` 页签的新增/编辑服务商对话框内：价格配置默认收起；点击每条模型映射行左侧的展开箭头（展开区同时含推理档位）后，可在价格区维护 输入 / 缓存读 / 缓存写 / 输出 四档单价（美元/百万 tokens），并可勾选启用 UTC+8 峰谷时段；每个峰谷时段可配置多段、每段四档优惠单价与生效星期（`0` 周日–`6` 周六，可多选；不选任何星期表示每天），多段按列表顺序取首个「星期与时刻」都命中的时段
- 四档全空表示该模型未定价、不产生价格行；任一档有值即为已定价，留空的档按 `0` 参与计价，四档都显式填 `0` 也算已定价（成本为 `0.0000`）
- 同一上游模型被多条映射行使用时共享同一份价格；保存服务商时把服务商与其完整价格行集合在一次 `api_gateway_upsert_provider` 调用中原子写入，删除映射或删除服务商时对应价格行同步移除
- 计价只按本次实际转发服务商自身、与上游模型名精确匹配（大小写敏感）的价格行，其他服务商的价格行或旧的全局价格行都不会参与；未定价请求在用量统计中显示 `—` 且不计入合计
- 旧版按上游模型名为键维护、不带服务商的全局价格行会在读取配置时幂等迁移：能匹配到某服务商映射上游模型或默认模型的行迁移为该服务商的专属价格行，匹配不到或已不可达的行被删除，并在下一次写入时持久化；`用量统计` 页签不再提供 `模型价格` 按钮与弹窗
- 默认模型为下拉选择，只列出该服务商已映射的上游模型（含空选项）；打开对话框时，旧的默认模型若没有任何映射覆盖，会自动补出一条带标记的映射行并保留其已有价格行，转发目标不变
- 金额在请求记录时固化，保存或迁移价格只影响之后的新请求，既有日志与已记录的金额永不改变

数据边界与清理：

- 日志只来自本机网关的转发流量，存于独立 SQLite 文件，不记录请求/响应正文、请求头或任何凭据，也不与 `Protocol Router`、`AI Environments`、`AI 用量统计` 共享数据
- 取消记录的隐藏不执行数据库迁移或历史删除；旧行只会按既有保留策略在到期清理时删除
- 保留天数在设置页的 `AI Gateway` 分区配置；每次写入新日志时会永久删除超期记录，删除不可恢复

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
- `AI Gateway`
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
- Antigravity：`agy`
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

### 22.11 AI Gateway

`AI Gateway`（AI 网关）分区只配置请求日志的保留天数：

- 默认 90 天，可填 1–365 的整数；输入 0、400 等非法值会拒绝保存并给出可操作错误，已存储的保留天数不被改写
- 该分区按分区独立保存与重置，保存与重置只作用于保留天数，不改写其他设置分区的草稿，也不改写网关的服务商、本地 Key 或终端同步配置
- 每次写入新日志时会永久删除超过保留天数的记录，删除不可恢复；缩短保留天数只影响之后的清理，不追溯修改已记录的历史金额

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
3. `Claude` / `Codex` / `Antigravity` 的 `Env Managed` 是否开启

### Q3：为什么 `onespace resume <session_id>` 可以恢复不同工具的会话

因为 `onespace resume` 是统一入口。

它会先从 OneSpace 当前的会话状态里找到这条会话，进入保存时的工作目录，再按工具转成各自的原生命令，例如：

- Claude -> `claude -r`
- Antigravity -> `agy --conversation <id>`
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
