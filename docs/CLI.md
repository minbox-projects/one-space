# OneSpace 快捷会话 CLI 使用文档

OneSpace 提供了一个原生的命令行工具（CLI），允许开发者在任何终端应用（如 iTerm2、Terminal 等）中，无需打开 OneSpace GUI 界面，就能在当前项目目录下快速创建并拉起 AI 会话。

所有的命令行创建的会话会自动同步显示在 OneSpace 客户端的“AI Sessions”列表中。

## 安装

1. 打开 OneSpace 桌面客户端。
2. 导航至左侧的 **AI Sessions** (AI 会话) 菜单。
3. 点击右上角的 **Install CLI** (安装 CLI) 按钮。
4. 系统会弹出提示“CLI tool installed to ~/.local/bin/onespace”。
5. *(可选)* 如果执行命令提示找不到 `onespace`，请确保您的 shell 配置文件（如 `~/.zshrc` 或 `~/.bashrc`）中包含了 `~/.local/bin` 路径：
   ```bash
   export PATH="$HOME/.local/bin:$PATH"
   ```

## 基本用法

打开您的终端，进入任意工作目录，使用以下语法：

```bash
onespace ai <模型简称> [会话名称]
```

### 参数说明

- `<模型简称>` **(必填)**: 指定启动 AI 会话时使用的模型和底层命令。
  - `claude`：启动 `claude code` 命令。
  - `gemini`：启动 `gemini -y` 命令。
  - `codex`：启动 `codex` 命令。
  - `opencode`：启动 `opencode` 命令。
  *(注意：通过 CLI 创建后，AI 会话记录会自动同步到 OneSpace 客户端。)*

- `[会话名称]` **(选填)**: 给此会话指定一个名字。
  - 如果省略此参数，系统将自动使用当前所在的**文件夹名称**，并自动加上 `_ai` 后缀。例如，如果当前在 `onespace-app` 文件夹中执行，创建的会话名将自动为 `onespace-app_ai`。
  - 会话名称不支持空格和点号（`.`），如果有，会自动转换为下划线（`_`）。

## 使用示例

### 1. 默认名称创建

在当前目录下快速启动一个基于 Claude 的 AI 会话：

```bash
cd ~/Projects/my-awesome-app
onespace ai claude
```
> **结果**: 会在后台创建一个名为 `my-awesome-app_ai` 的会话，并运行 `claude code`。

### 2. 指定名称创建

在当前目录下创建一个名为 "backend_refactor" 的 Gemini 会话：

```bash
onespace ai gemini backend_refactor
```
> **结果**: 会在后台创建一个名为 `backend_refactor` 的会话，并运行 `gemini -y`。

### 3. 使用 Codex 并传递额外参数

当您需要给底层命令传额外参数时，请显式提供会话名称，再追加参数：

```bash
onespace ai codex backend_refactor --model gpt-5
```

> **结果**: 会创建会话 `backend_refactor`，并执行 `codex --model gpt-5`。

## 会话管理

通过 CLI 创建的会话，您可以：
1. **直接运行**: AI 会话将在当前终端窗口中直接运行。
2. **在 OneSpace 客户端中管理**: 打开 OneSpace 客户端的 AI Sessions 面板，您会看到该会话的记录。
   - **Continue (继续)**: 以后可以在 OneSpace 中一键重新打开该特定会话。
   - **Remove (移除)**: 从 OneSpace 列表中移除该会话记录。

## 环境管理

OneSpace 允许您通过 CLI 快速切换特定工具（如 `claude` 或 `gemini`）使用的底层 AI 环境（Providers）。

### 查看环境列表

列出所有已配置的环境及其对应的工具，并显示当前处于活动状态的环境：

```bash
onespace env list
```

### 切换活动环境

将指定工具切换到另一个环境。可以使用环境名称或 ID。

```bash
onespace env use <工具名称> <环境名称或ID>
```

#### 参数说明
- `<工具名称>`: 您要切换环境的 AI 工具名。例如：`claude`、`gemini`、`codex`。
- `<环境名称或ID>`: 您在 OneSpace 客户端中配置的环境名称。

#### 使用示例

将 Claude 工具切换到名为 "Personal_Anthropic" 的环境：

```bash
onespace env use claude Personal_Anthropic
```

将 Codex 工具切换到名为 "work_openai" 的环境：

```bash
onespace env use codex work_openai
```

---
*注：通过 CLI 切换环境后，配置会立即同步。随后通过 CLI 或客户端启动的新会话将自动使用新环境。*
