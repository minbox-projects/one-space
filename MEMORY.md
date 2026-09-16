# Project memory

OneSpace 是面向开发者的 macOS 桌面工作台（Tauri 2 + React 19 + TypeScript），把 AI CLI 环境、原生终端会话、MCP、Skills/Subagents、工作流与常用生产力工具收拢到一个窗口。本文件是原生 agent 共享的架构与约定基线；`.ai-workflow/index/navigation.json` 是权威导航索引，`navigation.md` 由其生成。

## 技术栈与构建

- 前端：React 19、TypeScript、Vite 7、Tailwind CSS 3、Radix UI、i18next（中/英）。
- 后端：Rust、Tauri 2（`src-tauri/`），插件含 dialog / shell / process / updater / global-shortcut。
- 前端命令封装集中在 `src/lib/*`，通过 `@tauri-apps/api` 的 `invoke` 调用后端；UI 组件不直接拼装命令字符串。
- 构建命令：`npm run dev`、`npm run build`（`tsc -b && vite build`）、`npm run tauri dev`、`npm run tauri build`。
- 检查命令：`npm run lint`（eslint）、`npm test`（vitest run）、`npm run check:cli-matrix`；后端测试在 `src-tauri/` 下用 `cargo test`。
- 版本号三处保持一致：`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`。

## 模块根与职责

导航索引登记四个模块根，feature 的 `owner_role` 必须与所属模块根一致：

- `src`（owner `frontend`）：React 界面、状态、Tauri 命令封装与 i18n。
- `src-tauri/src`（owner `backend`）：Rust 命令、存储引擎、运行时与外部集成。
- `docs`（owner `documentation-maintainer`）：用户手册、CLI/MCP/Skills 文档与设计报告。
- `tools`（owner `backend`）：本地校验脚本。

## 前端架构（`src/`）

- `src/App.tsx` 是外壳与总控：侧边栏、页面切换、全局状态与快捷键；`src/lib/navigation.ts` 负责旧标签到新导航目标的解析（`resolveNavigationTarget`）。
- 每个业务域是一个 `src/components/<Domain>/` 目录或同名组件；每个组件目录通常包含 `index.tsx`、子组件、`*.test.tsx`，复杂域再拆分 `components/`、`hooks/`、`helpers/`、`types.ts`（参见 `Workspaces/`）。
- 命令封装与领域类型放在 `src/lib/`，按域一文件（如 `workflows.ts`、`skills.ts`、`subagents.ts`、`sshTunnels.ts`、`fileSharing.ts`、`shortLink.ts`、`aiAssistant.ts`、`apiFusion.ts`）。
- 文案统一走 `src/i18n.ts`，新增界面文本必须同时提供中英文；`en_keys.txt` / `zh_keys.txt` 为键清单。
- 共享基础组件在 `src/components/ui/`，Provider（主题、Toast、确认框、错误边界）在 `src/components/` 顶层。

## 后端架构（`src-tauri/src/`）

- `lib.rs` 声明模块并由 `app_runtime::run` 启动；`app_runtime/` 负责窗口、托盘、全局快捷键、CLI 入口与 OAuth。
- 每个业务域一个根文件加同名子目录（如 `ai_sessions.rs` + `ai_sessions/`、`skills.rs` + `skills/`、`protocol_router.rs` + `protocol_router/`）。子目录按 `commands`、`types`、`runtime`、`tests` 等拆分。
- `app_store/` 是统一存储与迁移核心：`storage_engine.rs`、`migration.rs`、`provider_projection/`、`sync.rs`、`types/`；会话、provider 与 launcher 命令都在此汇聚。
- 配置与密钥：`config.rs`、`runtime_profiles.rs`、`claude_profiles.rs`、`secrets.rs`、`crypto.rs`。
- CLI 探测与版本：`cli_probe.rs`、`cli_updates.rs`、`version_detect.rs`。

## Skills 统一目录与兼容

- `~/.agents/skills` 是所有 Skills 安装、扫描、同步与显示的规范目录；迁移后不再按工具维护独立 Skills 目录。
- `~/.claude/skills` 是指向 `~/.agents/skills` 的兼容符号链接；若该路径被普通文件/目录或错误、损坏的符号链接占用，则保持原样并返回可操作的失败，绝不覆盖。
- 同名冲突以统一目录版本为准；工具特定版本备份到 `~/.agents/skills/.backups/<tool>/<skill>/<content-hash>/`，按来源工具、Skill 名与内容哈希做幂等键，重复初始化不产生重复备份。
- 兼容性矩阵由后端记录 Claude / OpenCode / Codex / Antigravity 对 `~/.agents/skills` 的读取行为；无法直接读取的工具显式标记为依赖兼容路径或不受支持。

## API Fusion 模块与边界

- API Fusion 是独立模块：前端域 `src/components/ApiFusion/` 加命令封装 `src/lib/apiFusion.ts`，后端 `src-tauri/src/api_fusion.rs` 加同名子目录（`types_config`、`storage`、`selection`、`runtime_http`、`forwarding`、`commands`）。
- 导航 id 固定为 `api-fusion`（feature 归属模块根 `frontend`，owner `frontend`）与 `api-fusion-backend`（模块根 `tauri-backend`，owner `backend`）。`api-fusion` 是左侧「AI 能力」分组的顶层页签，位于 `ai-environments` 与 `ai-usage` 之间；`resolveNavigationTarget("api-fusion")` 解析为顶层 tab，不再归入 More Tools，More Tools 卡片与 Launcher 入口保留。新增工具 id 必须同时接入 `navigation.ts`、`moreToolPresentation.ts`、`launcherToolVisibility.ts`、`MoreToolsHub.tsx`、`Launcher.tsx` 与 `App.tsx`，否则页签不可达或启动器清单不一致。
- 每个上游服务商带 `protocol` 字段（`chat_completions` 默认 / `responses`）。中继接受 `/chat/completions`、`/responses` 及其无 `/v1` 形式并统一成 `/v1/...` 上游路径；候选选择按 `protocol` 过滤，协议不匹配的服务商不作为候选，且不做请求体转换，调用方需让客户端协议与服务商协议一致。
- 新建本地 Key 只需名称，值由后端用 OS 熵随机生成（`sk-fusion-<128bit hex>`）；编辑既有 Key 时留空或回传脱敏占位符保留原值。
- 本地服务固定监听 `127.0.0.1` 加配置端口（默认 `17688`），bind 失败即返回包含端口与原因的可操作错误，不回退到其他端口；启用状态持久化，重启后按上次状态自动恢复监听。
- 上游服务商、本地 Key 与终端同步台账保存在独立加密文件 `api_fusion.json`（经 `crate::crypto` 加密并临时文件加 rename 原子写入），与 Protocol Router、AI Environments 的存储互不共享，密钥不得以明文落盘。
- 终端写入边界：仅在用户主动“一键配置/同步”时写入 `tool` 为 `opencode`/`codex` 的既有服务商记录，且只替换 `base_url` 与 `api_key`（先读取既有记录再合并提交）；不改写 Protocol Router 的 route 数据，不触碰 `claude`/`antigravity` 记录。
- 失败分类与自动禁用集中在 `selection::classify_failure`：401/403 立即禁用，网络错误、非 JSON 响应体与 5xx 连续失败达 3 次禁用，429/404 仅切换候选，其余 4xx 把上游错误回传调用方；`enabled` 表示用户意图、`auto_disabled` 表示运行状态，二者独立持久化，手动重新启用只清理运行状态。
- 上述架构决策的由来见本地 ADR；用 `ai-workflow adr list --project <root>` 查看 ADR 状态与主题，不要扫描目录或维护独立索引。

## 数据与存储不变量

- local-first：运行时以本地镜像为主，再按配置同步到 `local / iCloud / Git`。
- 主密码用于加密敏感数据（`secrets.rs` / `crypto.rs`），密钥不得写入日志或提交到仓库。
- 设置页按分区独立保存与重置；每个分区应保持幂等。
- 会话历史、用量与 CLI 配置文件由后端解析后回填前端，前端不直接读取这些文件。

## 代码约定

- 只做必要改动，匹配现有风格；不引入未使用的抽象或依赖。
- 新增库前先确认 `package.json` / `Cargo.toml` 已存在该依赖。
- 前端行为测试使用 vitest + @testing-library，测试文件与被测文件同目录；后端测试位于对应域的 `tests.rs` 或 `tests/` 子目录。
- 保持 `navigation.json` 与 `navigation.md` 同步，并确保 `ai-workflow context validate` 通过。

## 工作流约束

- Planning 逐条澄清业务影响问题并冻结 `spec.md` / `plan.md`；Plan-to-tasks 生成不可变任务文件。
- Coding 以 TDD 在项目内临时 worktree 实现单个已批准任务，完成后依次通过 Spec Review 与 Standards Review 才可合并。
- 架构、归属、公共符号、路径或工作流规则变化时，必须同步更新 `MEMORY.md` 与 `navigation.json` 并重新生成、校验 `navigation.md`。
- 架构、模块边界与归属、公共协议或 schema、跨领域标准、工作流或 agent 规则、难以回退的技术选型发生变更时，写一条本地 ADR 到 `.ai-workflow/adr/`（`NNNN-kebab-title.md`，编号单调递增且不复用）。ADR 记录决策历史（为什么），`MEMORY.md` 记录当前标准（怎么做），二者不一致即为缺陷，须在同一变更内一起更新。
