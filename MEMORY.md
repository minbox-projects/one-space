# Project memory

Describe architecture, module responsibilities, coding standards and invariants used by Standards Review.

---

# 项目上下文

OneSpace 是一个基于 Tauri 2 的桌面应用。前端使用 TypeScript、React 和 Vite，后端使用 Rust 与 Tauri；应用通过 Tauri commands 和前端 typed IPC facade 连接界面与本地能力。仓库同时包含 AI 环境、会话、工作台、Skills、Subagents、MCP、SSH、工作流及本地工具等领域模块。

## 领域术语

- **OneSpace**：本仓库的桌面应用及其前后端整体。
- **Tauri**：桌面运行时边界；Rust 后端注册 commands、插件、托盘和窗口能力，前端通过 Tauri API 调用。
- **typed IPC facade**：位于 `src/lib/` 的类型化前端封装，用于集中包装 Tauri command 调用与事件。
- **子系统迁移**：由拥有数据的后端子系统管理其 schema 演进；例如 `app_store` 负责自身数据迁移。

## 仓库约束

- 前端代码使用 TypeScript、React 19 与 Vite，入口为 `src/main.tsx`，应用外壳为 `src/App.tsx`。
- 后端代码使用 Rust 2021 与 Tauri 2，二进制入口为 `src-tauri/src/main.rs`，模块声明位于 `src-tauri/src/lib.rs`，运行时组装和 command 注册位于 `src-tauri/src/app_runtime/run_app.rs`。
- 前端访问 Tauri commands 或后端事件时，经 `src/lib/` 中对应的 typed IPC facade 集中封装，避免在视图中分散协议细节。
- SQLite schema 和 migration 由拥有数据的领域子系统管理；变更必须保持 schema、迁移和领域读写逻辑一致。
- 前端现有检查包括 `npm run test`、`npm run lint` 和 `npm run build`；Rust 后端使用 Cargo 工具链执行测试、编译检查和格式检查。

## 职责

- **前端视图**：`src/components/` 负责页面、功能视图和用户交互，`src/App.tsx` 负责应用外壳、导航与视图挂载。
- **typed facade**：`src/lib/` 负责前端类型、Tauri command 调用和事件订阅的集中封装，为视图提供稳定边界。
- **Tauri commands/runtime**：`src-tauri/src/app_runtime/` 负责应用运行时组装、command 注册、插件、托盘、窗口及运行期服务；各领域 command 负责参数接收并调用领域实现。
- **领域模块**：`src-tauri/src/` 下的 `ai_env`、`ai_assistant`、`ai_sessions`、`skills`、`subagents`、`mcp_servers`、`ssh_tunnels`、`workflows` 等目录负责各自业务规则与后端能力。

## 模块边界

| 边界 | 关键目录或入口 |
|---|---|
| 项目级上下文与 Review Standards | `MEMORY.md` |
| 前端启动与应用外壳 | `src/main.tsx`、`src/App.tsx` |
| 前端功能视图 | `src/components/` |
| 前端 typed IPC facade 与工具 | `src/lib/` |
| Tauri 二进制与模块入口 | `src-tauri/src/main.rs`、`src-tauri/src/lib.rs` |
| Tauri command 注册与运行时 | `src-tauri/src/app_runtime/run_app.rs`、`src-tauri/src/app_runtime/` |
| Rust 领域模块 | `src-tauri/src/` 下各领域目录与模块文件 |
| 应用存储与迁移 | `src-tauri/src/app_store/` |
| Tauri 配置与能力 | `src-tauri/tauri.conf.json`、`src-tauri/capabilities/`、`src-tauri/Cargo.toml` |
