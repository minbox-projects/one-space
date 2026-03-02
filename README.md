# OneSpace 🚀

OneSpace 是一个为开发者打造的全能工作台，旨在通过集成终端、AI 助手、服务器管理和生产力工具，提供一个无缝的单窗口开发体验。

## 🌟 核心功能

### 🤖 AI 环境管理 (AI Environments)
集成了深度定制的 AI 环境切换器（灵感来自 `cc-switch`），让您可以轻松管理和调度多个 AI CLI 工具的配置。

- **多供应商支持**：完美支持 Claude Code, Codex, Gemini CLI 和 OpenCode。
- **自动提取配置**：首次启动时自动从系统路径（如 `~/.claude/settings.json`, `~/.gemini/.env`, `~/.config/opencode/opencode.json` 等）提取已有配置，实现零成本迁移。
- **环境预设管理**：为每个工具创建无限个配置预设（Presets），支持一键切换 API Key、Base URL 和模型。
- **Claude 深度定制**：支持精细化的模型路由（Reasoning, Haiku, Sonnet, Opus）以及“危险模式”权限跳过设置。
- **OpenCode 供应商模式**：针对 OpenCode 采用增量管理模式，支持在主配置文件中自由添加、更新或移除不同的 Provider。
- **品牌识别**：界面展示各 AI 厂商官方真实图标，防止配置混淆。

### 💬 AI 终端会话 (AI Sessions)
基于 Tmux 的持久化 AI 助手终端管理。

- **多会话并存**：同时运行多个 Claude Code 或 Gemini 实例，互不干扰。
- **环境关联感知**：启动会话前自动显示当前激活的 API 环境，确保费用消耗在可控范围内。
- **模型图标显示**：会话列表中自动识别并显示正在使用的模型品牌。
- **后台持久运行**：即使关闭窗口，AI 进程依然在后台运行，随时可以“恢复 (Attach)”连接。

### 🖥️ SSH 服务器管理
内置专业级 SSH 客户端管理功能。

- **自动导入**：支持从 `~/.ssh/config` 自动发现并导入服务器配置。
- **连接历史**：记录常用服务器，支持一键重连。
- **身份验证**：完善的私钥/密码管理支持。

### 📝 开发者工具集
- **启动台 (Launcher)**：快速启动本地应用、文件夹或执行常用的 Shell 命令。
- **代码片段 (Snippets)**：跨语言的代码库，支持语法高亮与一键复制。
- **收藏夹 (Bookmarks)**：管理您的开发文档、Git 仓库或内网地址。
- **备忘录 (Notes)**：支持 Markdown 的沉浸式笔记体验。

### ☁️ 云端与通讯
- **阿里云盘**：内置文件管理器，支持文件的上传、下载与预览。
- **Gmail 邮件**：集成收件箱，实时接收重要通知并支持撰写邮件。

## 🛠️ 技术架构

- **前端**：React 19 + TypeScript + TailwindCSS + Lucide Icons
- **后端**：Rust + Tauri 2.0 (提供极高的系统权限与安全性)
- **底层通信**：Tmux (用于管理 AI 终端状态)
- **配置持久化**：原子级写入保护，确保在系统异常时配置文件不损坏。

## 🛠️ macOS 安装与运行 (macOS Installation & Troubleshooting)

### ⚠️ 解决 “OneSpace 已损坏” 错误 (Fix "OneSpace is damaged")

如果您在 macOS 上打开应用时遇到 **“OneSpace 已损坏，无法打开。 您应该将它移到废纸篓。”** 的错误提示，这是由于 macOS 对未签名应用的安全性检查（Gatekeeper）导致的。

请按照以下步骤解决：

1. 打开 **终端 (Terminal)**。
2. 输入以下命令（任选其一，推荐方法 2）：

   **方法 1 (精确移除隔离属性):**
   ```bash
   sudo xattr -rd com.apple.quarantine /Applications/OneSpace.app
   ```

   **方法 2 (简洁清除模式 - 推荐):**
   ```bash
   sudo xattr -cr /Applications/OneSpace.app
   ```

3. **关键说明**：
   - 如果您的应用不在“应用程序”目录，请在输入 `sudo xattr -cr `（**注意末尾有空格**）后，将 **OneSpace** 图标直接拖入终端窗口以自动获取正确路径。
   - 报错 `Not enough arguments` 是因为命令中漏掉了 **App 的路径**。请确保命令格式为：`命令` + `空格` + `App路径`。
4. 按回车键，根据提示输入您的开机密码（输入时字符不会显示），然后按回车。

---

If you encounter the error message **"OneSpace is damaged and can't be opened. You should move it to the Trash."** on macOS, this is caused by macOS Gatekeeper security policies for unsigned applications.

To fix this:

1. Open **Terminal**.
2. Run the following command (Method 2 is recommended):
   ```bash
   sudo xattr -cr /Applications/OneSpace.app
   ```
   *(Note: If the app is not in the Applications folder, type `sudo xattr -cr ` and drag the app icon into the terminal window.)*
3. Enter your system password when prompted and press Enter.

## 🚀 快速上手

### 开发环境准备
1. 确保系统中已安装 [Rust](https://www.rust-lang.org/)。
2. 安装 [Node.js](https://nodejs.org/)。
3. 安装 [Tmux](https://github.com/tmux/tmux)（用于 AI 会话功能）。

### 运行
```bash
# 安装依赖
npm install

# 启动开发模式
npm run tauri dev
```

### 构建
```bash
npm run build
npm run tauri build
```

## 🌍 国际化
OneSpace 完整支持中英文切换，您可以随时在底部菜单进行语言调整。

## 🎨 主题支持
内置深色、浅色及系统跟随模式，保护开发者视力。

---

*OneSpace - 让您的终端更有温度。*
