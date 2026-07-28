# 移除 OneSpace AI Flow 集成

计划 ID：`remove-ai-flow`

## 目标

- 从 OneSpace 的前端、Tauri 命令面、Rust 模块、测试、国际化文案、用户文档和代码导航索引中完整移除 AI Flow 功能。
- 让移除后的 OneSpace 不再发现、读取、写入、安装、更新、启动或打开任何外部 AI Flow 目录、脚本、缓存或 Git 仓库。
- 保持 OneSpace 其余 AI 能力可编译、可测试且行为不变，特别是 AI Sessions、Workspaces、AI Environments、Skills、Subagents 和 MCP Servers。

## 需求

### 功能结果

- 侧边栏不再显示 `AI Flow`，应用不再渲染 `ai-flow` 标签页，也不再接受该托盘导航目标。
- 前端不再包含 AI Flow 组件、类型、Tauri API 包装器或相关测试。
- Tauri 不再暴露任何 `ai_flow_*` 命令，Rust crate 不再编译 `ai_flow` 模块。
- 应用内文档、README、使用手册和 `.ai-work-flow/index/` 不再将 AI Flow 描述为 OneSpace 功能。
- 仓库不再包含用于忽略旧项目级 `/.ai-flow/` 目录的专用规则。

### 非目标

- 不删除、修改、迁移或探测 `~/.config/ai-flow`。
- 不删除、修改、迁移或探测 `$AI_FLOW_HOME` 指向的位置。
- 不删除、修改、迁移或探测 OneSpace 数据目录中的 `cache/ai-flow/repo`。
- 不删除、修改、迁移或探测当前仓库或其他项目中可能存在的 `.ai-flow/` 用户数据。
- 不卸载或清理 Claude、Codex、Gemini、OpenCode 等 CLI 的 skills、agents 或相关配置。
- 不删除或改变 OneSpace 的 Skills、Subagents、AI Sessions、Workspaces、AI Environments、MCP、工作流预设等独立功能。
- 不删除仅因 AI Flow 使用过、但仍被其他模块共享的依赖，包括 `serde_yaml`、`dirs`、`uuid`、`tokio`；不顺带整理 `package.json`、Cargo manifest 或锁文件。
- 不把仓库工作流目录 `.ai-work-flow/` 误认为待删除的 `.ai-flow/`；前者必须保留并维护导航索引和本计划。

## 实施决策

### 删除策略

采用直接移除，不保留隐藏入口、兼容代理、空实现或返回“功能已移除”的 `ai_flow_*` 命令。删除前端调用面与后端命令面后，不执行外部数据迁移或清理。实现过程中不得运行以受保护外部路径为目标的 `rm`、`mv`、`cp`、Git、安装、同步或探测命令。

### 按依赖顺序的文件级步骤

1. **建立变更边界，不访问外部数据**
   - 先运行 `git status --short`，记录已有工作区变更；不得还原或覆盖与本计划无关的用户改动。
   - 后续操作只针对下文列出的仓库跟踪文件。即使本地存在 `.ai-flow/`、`~/.config/ai-flow`、`$AI_FLOW_HOME` 或 `cache/ai-flow/repo`，也不得读取、删除、迁移或据此改变实施方案。

2. **断开前端可达入口：`src/App.tsx`**
   - 删除 `./components/AiFlow` 导入。
   - 从 `TRAY_NAV_TABS` 删除 `"ai-flow"`，使托盘或事件载荷不再把该 ID 作为可导航页面。
   - 从 `navigationGroups` 的 AI 能力分组删除 `id: "ai-flow"` 的侧边栏项。
   - 删除 `shouldRenderTab("ai-flow")` 对应的渲染分支和 `<AiFlow>` 挂载点。
   - 保留仍被其他界面使用的 `Waypoints` 图标导入；不要因删除导航项而误删其其他用法。

3. **删除前端实现与专属测试**
   - 删除 `src/components/AiFlow/index.tsx`。
   - 删除 `src/components/AiFlow/AiFlow.test.tsx`，随后删除空目录 `src/components/AiFlow/`。
   - 删除 `src/lib/aiFlow.ts`，从前端彻底移除 11 个 Tauri 调用包装器、AI Flow 数据类型和错误格式化逻辑。
   - 删除 `src/lib/aiFlow.test.ts`；不为已删除 API 保留或新增兼容测试。

4. **先注销后删除 Tauri 后端能力**
   - 在 `src-tauri/src/app_runtime/run_app.rs` 的 `invoke_handler` 中删除以下 11 个注册项：
     - `ai_flow::ai_flow_install_latest`
     - `ai_flow::ai_flow_health_check`
     - `ai_flow::ai_flow_projects_list`
     - `ai_flow::ai_flow_project_status`
     - `ai_flow::ai_flow_plan_content_get`
     - `ai_flow::ai_flow_config_get`
     - `ai_flow::ai_flow_config_save`
     - `ai_flow::ai_flow_launch_preview`
     - `ai_flow::ai_flow_launch_action`
     - `ai_flow::ai_flow_queue_create`
     - `ai_flow::ai_flow_open_path`
   - 在 `src-tauri/src/lib.rs` 删除 `mod ai_flow;`。
   - 删除 `src-tauri/src/ai_flow.rs`，连同其中的安装/更新、健康检查、项目发现、配置读写与备份、计划/分组/队列状态解析、会话启动预览与执行、路径打开逻辑及模块内测试一并移除。
   - 不用新的模块复刻任何 AI Flow 路径解析或外部进程/Git 访问行为。

5. **删除国际化资源：`src/i18n.ts`**
   - 删除英文和中文资源中的 `docsAiFlowSummary`。
   - 删除英文资源中从 `aiFlowProjectNotInitialized` 到 `aiFlowWorkingDirectoryRequired` 的整组 AI Flow 专属键。
   - 删除中文资源中同名的整组 AI Flow 专属键。
   - 不删除共享的 `launch`、`create`、`workingDirectory` 等通用键；以 `aiFlow` 前缀和已确认的 `docsAiFlowSummary` 为删除边界。

6. **同步应用内文档与用户文档**
   - `src/components/Documentation.tsx`：删除 `id: 'ai-flow'` 的文档卡片；保留仍由 Protocol Router 使用的 `Route` 图标导入。
   - `docs/USAGE.md`：完整删除现有“10. AI Flow”及其 10.1 至 10.3 小节；将原第 11 至 25 章顺次重编号为第 10 至 24 章，并同步所有对应子章节编号。
   - `src/components/Documentation.tsx`：配合使用手册重编号，更新所有原第 11 章之后的 `usage` 锚点。已确认需调整的入口包括 Launcher/More Tools、SSH、Protocol Router、Snippets/Bookmarks/Notes、AI News、Mail、Cloud Drive、Fish Pond、Settings 和常见问题；锚点文本必须与 Markdown 新标题生成的 ID 一致。
   - `README.md`：从配套工具列表删除 `AI Flow`，并删除 Workspaces/AI Workspace 概览中的 AI Flow 功能条目；保留通用 AI Sessions、Workflow Presets 和其他工作流描述。

7. **删除仓库专属忽略规则：`.gitignore`**
   - 删除 `/.ai-flow/` 条目，因为 OneSpace 仓库不再提供或生成该功能目录。
   - 此修改只移除 Git 忽略规则，不得删除磁盘上可能已经存在的 `.ai-flow/`。若该目录因此出现在 `git status`，将其视为受保护的用户数据，不暂存、不修改、不纳入清理。

8. **维护代码导航索引**
   - `.ai-work-flow/index/feature-navigation.md`：删除 AI Flow 功能行。
   - `.ai-work-flow/index/frontend-navigation.md`：删除 `ai-flow` 侧边栏映射、AI Flow 功能模块行和 AI Flow 工具库行。
   - `.ai-work-flow/index/backend-navigation.md`：删除 `ai_flow` 领域模块行。
   - 保留 `.ai-work-flow/` 本身及其他所有功能索引；不要重排或改写无关条目。

### 关键删除点

| 层级 | 删除点 | 删除后的约束 |
|---|---|---|
| 导航 | `ai-flow` 托盘目标、侧边栏项、标签页渲染 | 用户和内部导航均不能再挂载 AI Flow 页面 |
| 前端 API | `src/lib/aiFlow.ts` 的 11 个 invoke 包装器 | 前端产物中不存在 `ai_flow_*` 调用 |
| Tauri 命令 | `run_app.rs` 的 11 个命令注册 | IPC 面不再公开 AI Flow 能力 |
| Rust 实现 | `src-tauri/src/ai_flow.rs` 与 `mod ai_flow` | 应用不再包含外部目录、脚本、缓存或 Git 仓库访问逻辑 |
| 内容 | i18n、Documentation、README、USAGE | UI 和文档不再宣称或引导使用 AI Flow |
| 导航索引 | 三个 `.ai-work-flow/index/*.md` 条目 | 后续代码导航不会指向已删除文件 |
| 仓库规则 | `.gitignore` 的 `/.ai-flow/` | 不再保留旧功能专属仓库规则，同时不触碰现有用户数据 |

## 接口与数据约束

- 这是 API 删除：上述 11 个 Tauri 命令无需兼容期，调用这些命令的旧前端代码必须与命令同时删除。
- 不新增替代 IPC、重定向、弃用提示或数据清理接口。
- 不读取外部 AI Flow 数据来决定删除范围；外部数据是否存在未知，也不影响验收。
- 不修改任何 AI Flow 外部数据的权限、时间戳、Git 状态、目录结构或内容。
- 保留 AI Sessions、Workspaces、AI Environments、Skills、Subagents、MCP 的前后端接口及数据模型。
- 保留共享 Rust/前端依赖和锁文件，除非另有独立计划证明其在全仓库已无用途；本计划不做该判断。

## 测试调整

- 删除 `src/components/AiFlow/AiFlow.test.tsx` 和 `src/lib/aiFlow.test.ts`，因为被测组件与 API 均被删除。
- `src-tauri/src/ai_flow.rs` 中的单元测试随模块删除，不迁移为其他模块测试。
- 若现有通用导航或 Documentation 测试明确枚举菜单/锚点，只删除 AI Flow 预期并按新章节编号更新锚点预期；不得放宽其他功能断言。任何此类额外文件必须先由残余引用扫描或测试失败精确指出，不能凭猜测批量修改。
- 不删除、跳过或弱化 AI Sessions、Workspaces、AI Environments、Skills、Subagents、MCP 及其他共享测试。

## 验证

### 静态与测试命令

按以下顺序执行，并修复由本次删除直接造成的问题：

1. `git diff --check`
2. `npm test`
3. `npm run lint`
4. `npm run build`
5. `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
6. `cargo test --manifest-path src-tauri/Cargo.toml`
7. `cargo check --manifest-path src-tauri/Cargo.toml`

不需要启动浏览器或访问任何外部 AI Flow 资源。构建和测试不得以外部 AI Flow 数据存在为前提。

### 残余引用扫描

完成删除后，从仓库根目录运行：

```bash
git grep -n -i -E 'ai[-_ ]?flow|aiFlow|AI_FLOW_HOME|cache/ai-flow/repo|\.ai-flow' -- ':!.ai-work-flow/plans/remove-ai-flow.md'
```

预期无输出并以“未找到匹配”结束；本计划文件因记录删除边界而明确排除。若有命中：

- 产品源码、测试、文案、用户文档、导航索引、manifest、脚本或仓库配置中的命中均必须逐项判断并清除。
- `.ai-work-flow/` 不是 `.ai-flow/`，不得因名称相近而删除工作流基础设施。
- 不通过扫描或遍历未跟踪的 `.ai-flow/`、用户主目录、`$AI_FLOW_HOME` 或 OneSpace 数据目录来证明完成。

### 差异边界检查

- 运行 `git diff --name-status`，确认删除文件仅为 `src/components/AiFlow/index.tsx`、`src/components/AiFlow/AiFlow.test.tsx`、`src/lib/aiFlow.ts`、`src/lib/aiFlow.test.ts`、`src-tauri/src/ai_flow.rs`。
- 确认修改文件仅为 `src/App.tsx`、`src-tauri/src/lib.rs`、`src-tauri/src/app_runtime/run_app.rs`、`src/i18n.ts`、`src/components/Documentation.tsx`、`README.md`、`docs/USAGE.md`、`.gitignore` 和三个 `.ai-work-flow/index/*.md`，外加本计划文件。
- 确认 `package.json`、包锁文件、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` 以及受保护功能文件没有变化。
- 若 `.gitignore` 修改后有未跟踪 `.ai-flow/` 出现在状态中，确认其未被暂存、修改或删除。

### 验收标准

- OneSpace UI、托盘导航和应用内文档均无 AI Flow 入口。
- 前端构建产物不再引用 `AiFlow`、`aiFlow`、`.ai-flow` 或 `ai_flow_*`。
- Tauri 命令注册中不存在上述 11 个命令，crate 中不存在 `ai_flow` 模块声明或实现文件。
- README、使用手册和代码导航索引不存在 AI Flow 功能说明；使用手册章节连续，应用内锚点可对应到重编号后的标题。
- 残余引用扫描除明确排除的本计划外无命中。
- 所列 npm 和 Cargo 验证全部通过，且未通过跳过测试或删除无关断言达成。
- 差异不包含外部用户数据、共享依赖清理或 Skills/Subagents/agents 等非目标内容。
- 验收不依赖外部 AI Flow 数据存在或已被清除；实施过程没有访问、修改、迁移或删除这些数据。

## 风险

| 风险 | 控制措施 |
|---|---|
| 只删页面但遗留 Tauri 命令，后台仍具备外部访问能力 | 同时删除前端包装器、11 个命令注册、模块声明和整个 Rust 实现，并运行残余扫描 |
| 删除 `.gitignore` 规则后本地 `.ai-flow/` 显现并被误处理 | 明确将其视为受保护用户数据，不暂存、不读取、不删除 |
| 将 `.ai-work-flow/` 与 `.ai-flow/` 混淆 | 只删除三个索引中的 AI Flow 行，保留工作流目录和计划文件 |
| 删除共享依赖导致其他模块构建失败 | manifest 与锁文件保持不变，并运行完整前端构建及 Cargo 测试/检查 |
| 使用手册重编号导致应用内跳转失效 | 同步更新 Documentation 中所有受影响的 usage 锚点，并核对标题生成 ID |
| 旧导航目标仍尝试打开已删除页面 | 从 `TRAY_NAV_TABS`、侧边栏和渲染分支同时删除 `ai-flow`；不保留隐藏挂载点 |
| 测试清理扩大到无关功能 | 只删除 AI Flow 专属测试；通用测试仅更新明确的菜单或锚点预期 |

## 范围外

- 外部 AI Flow 安装的卸载、缓存回收、仓库删除、配置迁移和项目目录清理。
- CLI skills/agents 的审计或清理。
- OneSpace Skills/Subagents、Workflow Presets 或 AI Sessions 的重构。
- 共享依赖瘦身、包升级、Cargo 特性调整和锁文件刷新。
- 为已删除功能提供兼容层、导出工具、迁移向导或弃用页面。

## 假设

- 已确认的 File Explorer 交接覆盖当前 AI Flow 产品实现的核心入口；实施后的仓库级残余扫描用于捕获遗漏的跟踪文件引用。
- 外部 AI Flow 数据可能存在，也可能不存在；两种情况都不需要验证，且不得改变实施行为。
- 删除属于有意的功能/API 破坏性变更，不要求兼容旧版前端或第三方对 `ai_flow_*` Tauri 命令的调用。
- `.gitignore` 中的 `/.ai-flow/` 是旧功能专属规则；删除规则不授权处理由此显现的任何本地目录。
