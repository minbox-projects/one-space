# 项目上下文

OneSpace 是一个基于 Tauri 2 的桌面应用。前端使用 TypeScript、React 和 Vite，后端使用 Rust 与 Tauri；应用通过 Tauri commands 和前端 typed IPC facade 连接界面与本地能力。仓库同时包含 AI 环境、会话、工作台、路由网关、Skills、Subagents、MCP、SSH、工作流及本地工具等领域模块。

## 领域术语

- **OneSpace**：本仓库的桌面应用及其前后端整体。
- **Tauri**：桌面运行时边界；Rust 后端注册 commands、插件、托盘和窗口能力，前端通过 Tauri API 调用。
- **AI 路由网关**：独立侧边栏功能模块，覆盖账号池、网关密钥、请求日志和设置等能力；前端入口位于 `src/components/AiRoutingGateway/`，后端位于 `src-tauri/src/ai_routing_gateway/`。
- **typed IPC facade**：位于 `src/lib/` 的类型化前端封装，用于集中包装 Tauri command 调用与事件；AI 路由网关的 facade 为 `src/lib/aiRoutingGateway.ts`。
- **共享 SQLite**：位于 `src-tauri/src/shared_sqlite/` 的共享数据库基础设施，供需要 SQLite 持久化的后端子系统使用。
- **子系统迁移**：由拥有数据的后端子系统管理其 schema 演进；例如 `app_store` 负责自身数据迁移，AI 路由网关通过共享 SQLite 基础设施管理其持久化数据。

## 仓库约束

- 前端代码使用 TypeScript、React 19 与 Vite，入口为 `src/main.tsx`，应用外壳为 `src/App.tsx`。
- 后端代码使用 Rust 2021 与 Tauri 2，二进制入口为 `src-tauri/src/main.rs`，模块声明位于 `src-tauri/src/lib.rs`，运行时组装和 command 注册位于 `src-tauri/src/app_runtime/run_app.rs`。
- 前端访问 Tauri commands 或后端事件时，经 `src/lib/` 中对应的 typed IPC facade 集中封装，避免在视图中分散协议细节。
- SQLite schema 和 migration 由共享数据库基础设施及拥有数据的领域子系统管理；变更必须保持 schema、迁移和领域读写逻辑一致。
- 前端现有检查包括 `npm run test`、`npm run lint` 和 `npm run build`；Rust 后端使用 Cargo 工具链执行测试、编译检查和格式检查。
- 功能入口或模块职责变化时，同步维护 `.ai-work-flow/index/` 中对应导航。

## 职责

- **前端视图**：`src/components/` 负责页面、功能视图和用户交互，`src/App.tsx` 负责应用外壳、导航与视图挂载。
- **typed facade**：`src/lib/` 负责前端类型、Tauri command 调用和事件订阅的集中封装，为视图提供稳定边界。
- **Tauri commands/runtime**：`src-tauri/src/app_runtime/` 负责应用运行时组装、command 注册、插件、托盘、窗口及运行期服务；各领域 command 负责参数接收并调用领域实现。
- **领域模块**：`src-tauri/src/` 下的 `ai_env`、`ai_assistant`、`ai_sessions`、`ai_routing_gateway`、`skills`、`subagents`、`mcp_servers`、`ssh_tunnels`、`workflows` 等目录负责各自业务规则与后端能力。
- **共享 SQLite**：`src-tauri/src/shared_sqlite/` 负责可复用的 SQLite 基础设施；使用它的子系统仍负责自身 schema、迁移与领域数据语义。
- **索引**：`.ai-work-flow/index/` 负责维护功能、前端和后端入口导航，不承载业务实现。

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
| AI 路由网关 | `src/components/AiRoutingGateway/`、`src/lib/aiRoutingGateway.ts`、`src-tauri/src/ai_routing_gateway/` |
| 共享 SQLite | `src-tauri/src/shared_sqlite/` |
| 应用存储与迁移 | `src-tauri/src/app_store/` |
| Tauri 配置与能力 | `src-tauri/tauri.conf.json`、`src-tauri/capabilities/`、`src-tauri/Cargo.toml` |
| 代码导航索引 | `.ai-work-flow/index/feature-navigation.md`、`.ai-work-flow/index/frontend-navigation.md`、`.ai-work-flow/index/backend-navigation.md` |

<!-- ai-team:project-context:start -->
<!-- ai-team:context-format {"renderer_version":"context-renderer-v2","schema_version":2} -->
## 项目上下文

### 项目形态
Tauri 2 desktop application with a React frontend and Rust backend

### 领域术语
- AI 路由网关账号池：统一管理 OAuth 与 API Key 账号；新增前选择类型，编辑按持久化 account_type 分流。

### 仓库约束
- OAuth enrollment 受 release gate 阻断；账号池前端不得新增 OAuth 写入 IPC，也不得修改 Rust、SQLite、Tauri command 或 typed IPC 契约。

### 职责
- src/components/AiRoutingGateway/index.tsx 负责账号池类型选择、API Key 创建表单、OAuth 不可用页和按持久化类型编辑分流。

### 模块边界
- API Key 创建继续经 src/lib/aiRoutingGateway.ts 的原子 facade；OAuth 编辑只写通用元数据，凭据、连接、映射和价格保持无写控件或只读。
<!-- ai-team:project-context:end -->
