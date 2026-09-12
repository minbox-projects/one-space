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
- 命令封装与领域类型放在 `src/lib/`，按域一文件（如 `workflows.ts`、`skills.ts`、`subagents.ts`、`sshTunnels.ts`、`fileSharing.ts`、`shortLink.ts`、`aiAssistant.ts`）。
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
