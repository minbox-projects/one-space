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
  - `opencode`：启动 `opencode` 命令。
  *(注意：通过 CLI 创建后，由于底层 tmux 会话限制，不允许在 OneSpace 客户端中更改模型类型。)*

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

## 会话管理

通过 CLI 创建的会话，您可以：
1. **在终端中附加**: 使用原生的 tmux 命令进入该会话：`tmux attach -t <会话名称>`
2. **在 OneSpace 客户端中管理**: 打开 OneSpace 客户端的 AI Sessions 面板，您会立刻看到新建的会话卡片。您可以点击卡片上的：
   - **Attach (附加)**: 打开系统的独立终端窗口进入该会话。
   - **Kill (终止)**: 强制结束该会话及其运行的 AI 进程。
   - **重命名图标 (✏️)**: 鼠标悬浮在卡片标题上可随时重命名会话（只允许修改名字，不允许修改模型类型）。

---
*注：该工具底层由 tmux 驱动，它会保证您的 AI 任务在后台持久化运行，即便关闭终端也不会丢失进度。*
