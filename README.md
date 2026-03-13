# OneSpace 🚀

OneSpace 是一个为开发者打造的全能工作台，旨在通过集成 AI 助手、服务器管理、自动化技能与智能体（Subagents）、MCP 协议支持以及丰富的生产力工具，提供一个无缝的单窗口开发体验。

## 🌟 核心功能

### 🤖 AI 环境与会话 (AI Environments & Sessions)
集成了深度定制的 AI 环境切换器，让您可以轻松管理和调度多个 AI CLI 工具的配置。

- **多供应商支持**：完美支持 Claude Code, Codex, Gemini CLI 和 OpenCode。
- **环境预设管理**：为每个工具创建无限个配置预设（Presets），支持一键切换 API Key、Base URL 和模型，并同步到系统 CLI 配置文件。
- **持久化会话**：直接在系统原生终端（如 macOS Terminal/iTerm2）中启动持久化的 AI 对话，支持会话恢复。
- **快速会话条**：全局快捷键唤起（默认 `Alt+Shift+A`），快速启动 AI 会话而无需切换窗口。

### 🧠 智能体与自动化 (Subagents & Workflows)
- **Subagents (子智能体) 管理**：一站式管理按模型（Claude/Gemini/Codex/OpenCode）划分的智能体预设，支持推荐源安装、本地导入与仓库更新。
- **工作流预设 (Workflow Presets)**：创建可复用的 AI 工作流启动模板。支持绑定特定的工作目录、模型、MCP Servers 以及必需的 Skills，提供 Strict（隔离）或 Shared（共享）启动作用域。
- **自动化执行与依赖检查**：一键检测并修复工作流所需的依赖（如缺失的 MCP 或未安装的 Skills）。

### 🛠️ 技能系统 (Skills)
- **多端同步与版本管理**：浏览、安装和更新跨不同 AI 模型（Claude/Gemini/Codex/OpenCode）的技能包。
- **技能源配置**：支持添加自定义 Git 仓库作为技能或 Subagents 源，并进行后台自动同步。

### 🔌 MCP 协议集成 (Model Context Protocol)
全面支持 Anthropic 推出的 MCP 协议，赋予 AI 助手访问本地文件系统、数据库、GitHub 等外部能力。

- **可视化管理**：新增、编辑、删除 MCP Server 配置。
- **模板快速创建**：内置 GitHub、PostgreSQL、Google Maps 等常用 MCP Server 模板。
- **多模型分配**：为不同的 AI 环境独立配置并启用所需的 MCP 服务。

### 🛠️ 开发者生产力工具集
- **SSH 服务器管理**：支持从 `~/.ssh/config` 自动导入，记录连接历史，支持私钥与密码管理。
- **启动台 (Launcher)**：快速搜索并启动本地应用、文件夹、执行 Shell 脚本或访问网址。
- **代码片段 (Snippets)**：跨语言代码库，支持语法高亮、标签管理与一键复制。
- **全能搜索 (OmniSearch)**：聚合检索会话、SSH、笔记、书签、技能等所有应用内资源。
- **备忘录与书签**：支持 Markdown 的沉浸式笔记体验与多分类的收藏夹。
- **配置备份 (Backup Manager)**：自动或手动创建 AI 工具配置的快照备份，支持一键恢复与过期数据清理。

### 🎮 游戏与压力释放 (Fun & Zen)
在繁重的开发间隙，提供多种经典游戏与解压工具。

- **经典游戏库**：扫雷 (Minesweeper)、贪吃蛇 (Snake)、数独 (Sudoku)、俄罗斯方块 (Tetris)、猜单词 (Wordle)。
- **电子禅意**：内置 **电子木鱼 (CyberMuyu)** 与 **电子鱼缸 (FishPond)**，助您平复心情，提升开发专注力。

### ☁️ 云端与通讯
- **阿里云盘**：内置文件管理器，支持文件预览与基础管理。
- **Gmail 邮件**：基于 OAuth 安全连接 Gmail，支持收件箱浏览与快速回复。

## 📸 软件截图

*(请参考 `screenshot/` 目录下的截图)*

## 📚 使用文档

- **完整使用手册**：[`docs/USAGE.md`](./docs/USAGE.md)
- **CLI 文档**：[`docs/CLI.md`](./docs/CLI.md)
- **Skills 文档**：[`docs/SKILLS.md`](./docs/SKILLS.md)
- **MCP 文档**：[`docs/MCP.md`](./docs/MCP.md)
- **应用内文档入口**：侧边栏 `Documentation`

## 🧭 推荐上手路径（5 分钟）

1. **初始化向导**：选择数据存储位置（Local / iCloud / Git）并设置主密码以加密敏感数据。
2. **配置 AI 环境**：进入 `AI Environments` 确认或导入 Claude/Codex/Gemini 配置。
3. **探索 Subagents 与 Workflows**：在工作流模块尝试创建一个整合了特定 MCP 和 Skills 的自动化流程。
4. **开启第一个会话**：在 `AI Sessions` 创建会话并在终端拉起。
5. **安装 CLI**：点击页面上的 `Install CLI` 以在终端中使用 `onespace` 命令。

## 🛠️ 技术架构

- **核心框架**：Tauri 2.0 (Rust) + React 19 + TypeScript
- **UI 风格**：Radix UI + TailwindCSS + Lucide Icons (现代化暗黑/明亮主题支持)
- **同步引擎**：原子级写入保护，支持 iCloud 与 Git 异地多机同步，以及数据精细化范围同步（Sync Policy）。

## 🖥️ macOS 安装与运行 (macOS Installation)

如果您在 macOS 上遇到 **“OneSpace 已损坏”** 的错误，这是由于 Gatekeeper 机制导致的。请在终端中执行以下命令解决：
```bash
sudo xattr -cr /Applications/OneSpace.app
```

## 🚀 快速上手 (Development)

1. 安装 [Rust](https://www.rust-lang.org/) 与 [Node.js](https://nodejs.org/)。
2. 克隆仓库并安装依赖：`npm install`
3. **运行开发版**：`npm run tauri dev`
4. **构建发行版**：`npm run tauri build`

## 🌍 国际化
OneSpace 完整支持中英文切换，适配全球开发者使用习惯。
