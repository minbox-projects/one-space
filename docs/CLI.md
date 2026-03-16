# OneSpace CLI 文档

OneSpace 自带一个轻量命令行工具 `onespace`，主要用来做两件事：

- 从当前终端目录快速创建 AI 会话
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
onespace ai <model_shortcut> [session_name] [extra args...]
onespace env list
onespace env use <tool> <provider_name_or_id>
```

## 3. `onespace ai`

### 3.1 基本语法

```bash
onespace ai <model_shortcut> [session_name] [extra args...]
```

支持的模型简称：

- `claude`
- `gemini`
- `codex`
- `opencode`

### 3.2 实际执行的底层命令

CLI 脚本当前内置映射如下：

- `claude` -> `claude code`
- `gemini` -> `gemini -y`
- `codex` -> `codex`
- `opencode` -> `opencode`

注意：

- 这里的命令映射来自 `onespace` 安装脚本本身
- 它与桌面端 `Settings -> AI Terminal` 中的“启动命令模板”不是同一套机制

### 3.3 会话名规则

如果你显式传入 `[session_name]`：

- 会作为 OneSpace 里的会话记录名
- 其中空格和点号 `.` 会被转换成下划线 `_`

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

脚本的第三个参数永远先被当成 `session_name`。

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

## 4. `onespace ai` 会做什么

执行 `onespace ai ...` 时，脚本会：

1. 读取当前工作目录
2. 生成一个 OneSpace 会话记录
3. 把记录写入 `ai_sessions.json`
4. 在当前终端直接执行目标 CLI

因此你会同时得到两件事：

- 当前终端里立刻启动 AI CLI
- 桌面应用 `AI Sessions` 页面里出现对应记录

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

### 5.2 自定义会话名启动 Gemini

```bash
onespace ai gemini backend_refactor
```

效果：

- 会话名是 `backend_refactor`
- 当前终端执行 `gemini -y`

### 5.3 传递额外参数给 Codex

```bash
onespace ai codex api_cleanup --model gpt-5
```

效果：

- 会话名是 `api_cleanup`
- 当前终端执行 `codex --model gpt-5`

## 6. `onespace env list`

查看 OneSpace 当前记录的环境快照与活动绑定：

```bash
onespace env list
```

输出内容大致包括：

- 所有环境名称与所属工具
- 每个工具当前的活动环境

这个命令读取的是 OneSpace 的 `providers.json` 快照，不会主动扫描系统 CLI 配置文件。

## 7. `onespace env use`

切换某个工具对应的活动环境：

```bash
onespace env use <tool> <provider_name_or_id>
```

示例：

```bash
onespace env use claude Personal_Anthropic
onespace env use codex work_openai
```

### 7.1 这个命令当前的真实作用

它会更新 OneSpace 记录里的“活动环境绑定”。

更准确地说：

- 会修改 OneSpace 的 `providers.json`
- 会把某个工具的 `active_<tool>` 指向新的 provider

### 7.2 需要特别注意的限制

`onespace env use` 当前不会像桌面端 `Apply to CLI` 那样主动重写目标 CLI 配置文件。

所以如果你的目标是：

- 让 `Claude` / `Codex` / `Gemini` 的实际 CLI 配置立即切换

推荐做法仍然是：

1. 在桌面端 `AI Environments` 中选择目标环境
2. 点击 `Apply to CLI`

可以把 `env use` 理解为：

- 主要更新 OneSpace 内部的活动环境状态
- 适合做快速切换标记
- 不应把它当成完整的配置投影命令

## 8. CLI 与桌面端的分工

推荐把二者分开理解：

- 桌面端负责：
  - 环境编辑
  - 配置投影
  - 工作流
  - MCP / Skills / Subagents 管理
  - 会话浏览与恢复
- CLI 负责：
  - 终端内快速创建会话
  - 简单查看或切换活动环境绑定

## 9. 常见问题

### Q1：为什么 `onespace` 命令存在，但桌面端里没看到新会话

通常按顺序排查：

1. 是否从 OneSpace 安装过 CLI，而不是旧脚本
2. 当前目录是否可访问
3. 目标 CLI 是否真的成功启动
4. 切回桌面端后等待几秒，让历史同步补齐

### Q2：为什么 `onespace env use` 后 CLI 好像没变化

因为当前 `env use` 主要更新 OneSpace 内部活动环境映射，不等同于桌面端的 `Apply to CLI`。

### Q3：Claude、Gemini、Codex、OpenCode 的恢复命令分别是什么

可参考：

[`docs/AI_SESSION_COMMAND_MATRIX.md`](./AI_SESSION_COMMAND_MATRIX.md)
