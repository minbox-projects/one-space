# OneSpace CLI 文档

OneSpace 自带一个轻量命令行工具 `onespace`，主要用来做三件事：

- 从当前终端目录快速创建 AI 会话
- 通过统一入口恢复已存在的 AI 会话
- 查看或切换 OneSpace 记录的活动环境绑定

这不是一个完整的桌面端替代品。它更像桌面应用的终端入口。

## 1. 安装

在桌面应用中进入 `AI Sessions`，点击右上角 `Install CLI`。

默认安装路径：

```bash
~/.local/bin/onespace
```

如果命令找不到，把这一路径加入 `PATH`：

```bash
export PATH="$HOME/.local/bin:$PATH"
```

## 2. 命令总览

```bash
onespace --help
onespace ai <model_shortcut> [session_name] [extra args...] [--permission-mode default|full_access]
onespace resume <session_id> [--permission-mode default|full_access]
onespace env list
onespace env use <tool> <provider_name_or_id>
```

## 3. `onespace ai`

### 3.1 基本语法

```bash
onespace ai <model_shortcut> [session_name] [extra args...] [--permission-mode default|full_access]
```

支持的模型简称：

- `claude`
- `antigravity`
- `codex`
- `opencode`

### 3.2 实际执行的底层命令

CLI 脚本不再内置命令映射，而是把参数原样交给 OneSpace 后端的共享会话服务解析并执行。默认底层命令为：

- `claude` -> `claude --session-id <session_id>`
- `antigravity` -> `agy`
- `codex` -> `codex`
- `opencode` -> `opencode`

说明：

- 这些默认命令由后端 `ai_sessions::build_create_command` 决定
- 它会读取桌面端 `Settings -> AI Terminal` 的“启动命令模板”（`ai_model_launch_commands`）并覆盖默认值，两者现在是同一套机制
- 完全访问权限模式会在启动命令后追加对应工具的权限参数
- 旧版脚本自行拼装命令并写会话 JSON 的行为已移除

### 3.3 会话名规则

如果你显式传入 `[session_name]`：

- 会原样作为 OneSpace 里的会话记录名（空格与点号不再被改写）

如果不传 `[session_name]`：

- 会使用当前文件夹名
- 自动追加 `_ai`

例如当前目录是：

```bash
~/Projects/onespace-app
```

执行：

```bash
onespace ai codex
```

会生成会话名：

```text
onespace-app_ai
```

### 3.4 额外参数的解析规则

`onespace ai` 的第一个参数是模型简称，第二个参数（如果存在）永远先被当成 `session_name`。

这意味着：

- 如果你要把额外参数传给底层 CLI
- 请先显式写一个会话名

正确示例：

```bash
onespace ai codex backend_refactor --model gpt-5
```

这会被解释为：

- 会话名：`backend_refactor`
- 底层命令：`codex --model gpt-5`

容易误解的写法：

```bash
onespace ai codex --model gpt-5
```

这会把 `--model` 当成会话名，而不是参数。

命名后的额外参数会在当前终端原样转发给底层 CLI。

### 3.5 `--permission-mode` 选项

`onespace ai` 与 `onespace resume` 都接受可选的 `--permission-mode default|full_access`，用于对本次操作显式确认权限模式：

- 工具配置为 `default` 时，传入 `full_access` 会被拒绝（`INVALID_PERMISSION_MODE`，不能自行提权）
- 工具配置为 `full_access` 时，必须显式选择 `default` 或 `full_access`；缺省会以 `PERMISSION_CONFIRMATION_REQUIRED` 拒绝且不启动、不改动状态
- 显式选择 `default` 会以非提权方式运行

对 `onespace ai`，该选项必须出现在工具简称之后（`--permission-mode <mode>` 成对出现），多余或缺失的值会以 `invalid_payload` 拒绝；它由后端解析并校验，不会作为会话名，也不会原样转发给原生 CLI。

## 4. `onespace ai` 会做什么

执行 `onespace ai ...` 时，会：

1. 读取当前工作目录
2. 解析工具、可选显示名、可选 `--permission-mode` 与其余原生参数
3. 经 OneSpace 后端共享的 canonical 会话服务注册一条会话记录（初始为 `pending_bind`）
4. 在当前终端直接执行目标 CLI，并继承当前终端的输入输出
5. 目标 CLI 退出后，把它的退出码原样返回给终端

因此你会同时得到两件事：

- 当前终端里立刻启动 AI CLI
- 桌面应用 `AI Sessions` 页面里出现对应记录

补充：

- 记录先以 `pending_bind` 存在，等 CLI 历史里出现真实原生会话 ID 后才绑定为活动记录；显示名不会冒充原生 ID
- 如果工具不支持、工作目录无效或进程无法启动，刚登记的记录会被回滚，不会留下误报为可恢复的会话
- 如果 CLI 成功启动但之后以非零状态退出，这属于正常的子进程退出结果，记录仍然保留

### 4.1 与桌面端会话列表的关系

CLI 创建出的记录会出现在桌面端中，但要理解下面这一点：

- 桌面端后续还会从各 CLI 历史记录里同步真实会话 ID、标题和模型
- 所以你最初看到的记录，可能稍后会被历史同步进一步补全

## 5. 使用示例

### 5.1 在当前目录启动 Claude

```bash
cd ~/Projects/my-app
onespace ai claude
```

效果：

- 会话名默认是 `my-app_ai`
- 当前终端执行 `claude code`

### 5.2 自定义会话名启动 Antigravity

```bash
onespace ai antigravity backend_refactor
```

效果：

- 会话名是 `backend_refactor`
- 当前终端执行 `agy --dangerously-skip-permissions`

### 5.3 传递额外参数给 Codex

```bash
onespace ai codex api_cleanup --model gpt-5
```

效果：

- 会话名是 `api_cleanup`
- 当前终端执行 `codex --model gpt-5`

## 6. `onespace resume`

### 6.1 基本语法

```bash
onespace resume <session_id> [--permission-mode default|full_access]
```

这里的 `session_id` 推荐直接使用桌面端 `AI Sessions` 里复制出来的 Session ID。

### 6.2 它会做什么

执行 `onespace resume ...` 时，会：

1. 通过 OneSpace 后端 canonical 会话服务解析目标会话
2. 若记录仍处于 pending/unbound 或缺少原生 ID，则先尝试按工作目录与时间窗从 native history 解析真实 ID；仍解析不到时以 `SESSION_ID_MISSING` 拒绝
3. 校验该原生 ID 未被同一工具的另一条会话占用（`SESSION_ID_CONFLICT`）
4. 先进入这条会话保存时的工作目录
5. 复用共享的恢复准备：工具原生恢复命令、provider/runtime 环境与权限模式
6. 在当前终端执行对应工具自己的原生恢复命令，并继承当前终端的输入输出

因此你在任意终端里执行的统一命令是：

```bash
onespace resume <session_id>
```

但实际底层命令会按工具分发：

- `claude` -> `claude -r <session_id>`
- `antigravity` -> `agy --conversation <session_id>`
- `codex` -> `codex resume <session_id>`
- `opencode` -> `opencode -s <session_id>`

### 6.3 查找规则

后端会优先按当前会话状态里的 `tool_session_id` 查找。

如果没找到，再兼容按 OneSpace 自己的会话 `id` 查找；但真正传给底层 CLI 的仍然是该会话对应的 `tool_session_id`。若该原生 ID 已被同一工具的另一条会话绑定，恢复会以 `SESSION_ID_CONFLICT` 拒绝。

### 6.4 使用示例

```bash
onespace resume 6a1f0c0d-xxxx-xxxx-xxxx-demo
onespace resume 6a1f0c0d-xxxx-xxxx-xxxx-demo --permission-mode default
```

效果：

- 自动识别这条会话属于哪个工具
- 先进入该会话原工作目录
- 执行该工具对应的恢复命令

## 7. `onespace env list`

查看 OneSpace 当前记录的环境快照与活动绑定：

```bash
onespace env list
```

输出内容大致包括：

- 所有环境名称与所属工具
- 每个工具当前的活动环境

说明：

- 读取的是 OneSpace 的 canonical 服务商状态（profile 应用目录下的加密 provider 状态），不会主动扫描系统 CLI 配置文件
- `Claude` / `Codex` / `Antigravity` 各显示一个活动环境
- `OpenCode` 按多活动集合展示，可能同时列出多个，也可能一个都没有（不会回退到遗留的单槽记录）

## 8. `onespace env use`

切换某个工具对应的活动环境：

```bash
onespace env use <tool> <provider_name_or_id>
```

示例：

```bash
onespace env use claude Personal_Anthropic
onespace env use codex work_openai
```

### 8.1 这个命令当前的真实作用

它会更新 OneSpace canonical 服务商状态里的“活动环境绑定”，与桌面端激活共用同一套后端原语。

具体行为：

- `Claude` / `Codex` / `Antigravity`：把该工具的 `active_<tool>` 指向新的 provider（单活动）
- `OpenCode`：把 provider 追加进活动集合，已有活动项保留（可多活动，重复项不重复追加）

### 8.2 需要特别注意的限制

`onespace env use` 当前不会像桌面端 `Apply to CLI` 那样主动重写目标 CLI 配置文件。

所以如果你的目标是：

- 让 `Claude` / `Codex` / `Antigravity` 的实际 CLI 配置立即切换

推荐做法仍然是：

1. 在桌面端 `AI Environments` 中选择目标环境
2. 点击 `Apply to CLI`

可以把 `env use` 理解为：

- 主要更新 OneSpace 内部的活动环境状态
- 适合做快速切换标记
- 不应把它当成完整的配置投影命令

## 9. CLI 与桌面端的分工

推荐把二者分开理解：

- 桌面端负责：
  - 环境编辑
  - 配置投影
  - MCP / Skills / Subagents 管理
  - 会话浏览与恢复
- CLI 负责：
  - 终端内快速创建会话
  - 终端内快速恢复会话
  - 简单查看或切换活动环境绑定

## 10. 常见问题

### Q1：为什么 `onespace` 命令存在，但桌面端里没看到新会话

通常按顺序排查：

1. 是否从 OneSpace 安装过 CLI，而不是旧脚本
2. 当前目录是否可访问
3. 目标 CLI 是否真的成功启动
4. 切回桌面端后等待几秒，让历史同步补齐

### Q2：为什么 `onespace env use` 后 CLI 好像没变化

因为当前 `env use` 主要更新 OneSpace 内部活动环境映射，不等同于桌面端的 `Apply to CLI`。

### Q3：为什么 `onespace resume <session_id>` 能恢复不同工具的会话

因为 `onespace resume` 本身只是统一入口。

它会先读取 OneSpace 保存的会话记录，再自动转成目标工具自己的恢复命令：

- Claude -> `claude -r`
- Antigravity -> `agy --conversation <id>`
- Codex -> `codex resume`
- OpenCode -> `opencode -s`

所以你记住一个统一命令即可：

```bash
onespace resume <session_id>
```

## 11. 会话权限模式（`--permission-mode`）

OneSpace 在设置页为每个终端工具（Claude Code、Antigravity、Codex、OpenCode）提供权限模式配置；CLI 的 `onespace ai` 与 `onespace resume` 都用 `--permission-mode` 对本次操作显式确认。

### 11.1 模式说明

| 模式 | 行为 | 适用场景 |
|------|------|----------|
| 默认权限 | 会话命令不追加任何权限参数，保持工具默认的安全行为 | 日常开发，需要工具逐项确认敏感操作 |
| 完全访问 | 新建或恢复时跳过工具权限确认或放宽权限控制 | 受信任的项目环境中需要高效操作 |

### 11.2 各工具完全访问参数

| 工具 | 权限参数 | 说明 |
|------|----------|------|
| Claude Code | `--dangerously-skip-permissions` | 跳过所有文件/命令权限确认 |
| Antigravity | `--dangerously-skip-permissions` | 跳过所有文件/命令权限确认 |
| Codex | `--dangerously-bypass-approvals-and-sandbox` | 跳过审批和沙箱 |
| OpenCode | `OPENCODE_PERMISSION=allow`（环境变量） | 临时放宽权限控制 |

### 11.3 安全边界

- `--permission-mode` 同时适用于新建（`onespace ai`）与恢复（`onespace resume`）会话
- 工具配置为完全访问时，缺少显式确认会以 `PERMISSION_CONFIRMATION_REQUIRED` 拒绝，且不启动、不改变状态；桌面端新建/恢复前仍会弹出确认弹窗，用户可降级为默认权限
- 后端会拒绝未显式确认的完全访问请求，防止任何入口绕过权限确认
- 显式选择 `--permission-mode default` 会以非提权方式运行
- 配置为默认权限的工具无法被调用方提升到完全访问
