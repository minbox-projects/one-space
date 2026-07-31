# OneSpace 后端 (Tauri / Rust) 结构导航

## 顶层结构

| 区域 | 职责 | 路径 |
|---|---|---|
| 应用入口 | 二进制入口 | `src-tauri/src/main.rs` |
| Tauri 构建 | 命令注册、托盘、窗口、插件 | `src-tauri/src/app_runtime/run_app.rs` |
| 模块声明 | 所有模块的 mod 声明 | `src-tauri/src/lib.rs` |

## Tauri 命令注册

所有命令在 `src-tauri/src/app_runtime/run_app.rs:156` 的 `invoke_handler` 中注册。

## 领域模块

| 模块 | 职责 | 入口文件 |
|---|---|---|
| `app_store` | 数据存储、投影、同步、迁移、CRUD | `src-tauri/src/app_store/` |
| `ai_env` | AI 环境（Claude/Codex/Gemini）管理 | `src-tauri/src/ai_env/commands.rs` |
| `ai_assistant` | AI 工作台对话、助手、自动化、模型 | `src-tauri/src/ai_assistant/commands.rs` |
| `assistant_mcp` | 助手 MCP 工具目录 | `src-tauri/src/assistant_mcp.rs` |
| `ai_sessions` | AI 终端会话、用量统计 | `src-tauri/src/ai_sessions/` |
| `ai_news` | AI 资讯 RSS 抓取与同步 | `src-tauri/src/ai_news.rs` |
| `skills` | Skills 源管理、安装、同步、更新 | `src-tauri/src/skills/commands.rs` |
| `subagents` | Subagents 源管理、安装、同步、更新 | `src-tauri/src/subagents/commands.rs` |
| `mcp_servers` | MCP Server CRUD、模型开关、更新 | `src-tauri/src/mcp_servers/commands.rs` |
| `mcp_templates` | MCP 内置模板 | `src-tauri/src/mcp_templates.rs` |
| `mcp_export` | MCP 导入导出 | `src-tauri/src/mcp_export.rs` |
| `mcp_runtime` | MCP 运行时 | `src-tauri/src/mcp_runtime.rs` |
| `ssh_tunnels` | SSH 隧道管理、转发、重连、状态 | `src-tauri/src/ssh_tunnels/commands.rs` |
| `ssh_oauth` | SSH 连接、Google OAuth | `src-tauri/src/app_runtime/ssh_oauth.rs` |
| `protocol_router` | 协议路由 CRUD、启停、状态 | `src-tauri/src/protocol_router/commands.rs` |
| `file_sharing` | 私有 IPv4 网卡发现、临时 HTTP 下载、Range、内存传输记录与退出清理 | `src-tauri/src/app_runtime/run_app.rs`（命令注册）、`src-tauri/src/file_sharing.rs`、`src-tauri/src/file_sharing/{runtime,http,types}.rs` | 只绑定用户选择的 RFC1918 IPv4；令牌、文件映射、传输记录均为进程内临时状态，`request_shutdown` 在托盘退出和 `RunEvent::Exit` 清理；不接入 Cloud Drive、同步、备份或存储 |
| `workflows` | 工作流预设 CRUD、启动、依赖 | `src-tauri/src/workflows/` |
| `workspaces` | 工作空间 CRUD、会话映射 | `src-tauri/src/workspaces.rs` |
| `config` | 应用配置持久化 | `src-tauri/src/config.rs` |
| `config_conflict` | AI 环境配置冲突检测 | `src-tauri/src/config_conflict.rs` |
| `storage` | 简单 KV 存储（书签/笔记/片段/游戏） | `src-tauri/src/storage.rs` |
| `messages` | 消息/通知系统 | `src-tauri/src/messages.rs` |
| `secrets` | 主密码加解密 | `src-tauri/src/secrets.rs` |
| `short_link` | TinyURL Token 状态、加密存取、URL 校验及短链创建命令 | `src-tauri/src/short_link.rs`、`src-tauri/src/secrets.rs`、`src-tauri/src/app_runtime/run_app.rs` |
| `backup` | 数据备份与恢复 | `src-tauri/src/backup.rs` |
| `proxy` | 系统代理配置 | `src-tauri/src/proxy.rs` |
| `crypto` | 加密工具 | `src-tauri/src/crypto.rs` |
| `git` | Git 同步 | `src-tauri/src/git.rs` |
| `claude_profiles` | Claude 配置文件处理 | `src-tauri/src/claude_profiles.rs` |
| `cli_probe` | CLI 安装检测 | `src-tauri/src/cli_probe.rs` |
| `cli_updates` | CLI 更新检查与应用 | `src-tauri/src/cli_updates.rs` |
| `version_detect` | 版本检测与配置兼容性 | `src-tauri/src/version_detect.rs` |
| `runtime_profiles` | 运行时 profile 管理 | `src-tauri/src/runtime_profiles.rs` |
| `managed_assets` | 受管资源 | `src-tauri/src/managed_assets.rs` |

## 运行时工具

| 模块 | 职责 | 路径 |
|---|---|---|
| `cli` | CLI 命令内部处理 | `src-tauri/src/app_runtime/cli.rs` |
| `shortcuts_tray` | 全局快捷键、托盘菜单、窗口操作 | `src-tauri/src/app_runtime/shortcuts_tray.rs` |
| `windows_data` | 窗口状态、数据目录 | `src-tauri/src/app_runtime/windows_data.rs` |
| `runtime_services` | HTTP 代理请求等服务 | `src-tauri/src/app_runtime/runtime_services.rs` |
| `oauth_open` | OAuth 流程、外部 URL 打开 | `src-tauri/src/app_runtime/oauth_open.rs` |

## 静态 / 配置

| 用途 | 路径 |
|---|---|
| Tauri 配置 | `src-tauri/tauri.conf.json` |
| Cargo 依赖 | `src-tauri/Cargo.toml` |
| 能力声明 | `src-tauri/capabilities/` |
| 构建脚本 | `src-tauri/build.rs` |
