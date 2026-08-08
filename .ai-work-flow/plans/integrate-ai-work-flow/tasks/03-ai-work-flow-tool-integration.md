# 03 - 完成更多工具前端、跨层集成与端到端验收

- task_id: `ai-work-flow-tool-integration`
- order: `03`
- blocked_by: `ai-work-flow-install-backend, ai-work-flow-environment-backend`
- source_plan: `../plan.md`
- source_plan_digest: `7b23b06dfa04e4ecabacf52563ad18c6ff2756ec02bc437d8fcfab6b6b28f577`
- plan_id: `integrate-ai-work-flow`
- preview_revision: `1`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src/lib/navigation.ts`
  - `src/lib/navigation.test.ts`
  - `src/lib/moreToolPresentation.ts`
  - `src/lib/launcherToolVisibility.ts`
  - `src/lib/launcherToolVisibility.test.ts`
  - `src/lib/aiWorkFlow.ts`
  - `src/lib/aiWorkFlow.test.ts`
  - `src/App.tsx`
  - `src/App.moreToolsNavigation.test.tsx`
  - `src/components/MoreToolsHub.tsx`
  - `src/components/MoreToolsHub.test.tsx`
  - `src/components/AiWorkFlowTool.tsx`
  - `src/components/AiWorkFlowTool.test.tsx`
  - `src-tauri/src/ai_work_flow.rs`
  - `src-tauri/src/app_runtime/run_app.rs`

## 预期结果

扩展更多工具导航类型、别名、展示元数据和选中状态，提供不读取、不展示且不链接本地 README 的静态 AI Work Flow 页面；实现安装状态与可选版本解耦、安装或更新、取消、阶段结果和结构化日志，缺少版本时成功流程仍保持成功且前端显示“已安装（版本未知）”，并实现环境列表、创建、选择、删除、切换和完整 JSON 读取编辑保存交互。补充导航、组件和前后端集成测试，验证无版本安装状态、重复操作禁用、错误展示、完整 JSON 编辑、default 回退、命令注册、跨层状态流及数据域隔离；最后执行 OneSpace 全量检查、受管理 AI Work Flow 仓库验收命令和首次安装、更新、失败、取消及环境操作的人工端到端验收。

## 实施清单

- [x] 将 `ai-work-flow` 加入 `MoreToolsSection`、别名解析、更多工具展示元数据和启动台可见性模型；使用现有 Lucide 图标与工具卡样式，并保持 `App.tsx`、直接导航和返回工具列表时的选中状态一致。
- [x] 在 `MoreToolsHub` 加入 AI Work Flow 工具卡和专用内容分支；页面首屏是可操作工具界面，静态简介由前端内置，不读取、不解析、不展示且不链接本地或受管理仓库 README。
- [x] 在 `src/lib/aiWorkFlow.ts` 定义与后端序列化结果一致的类型和集中 invoke 封装，覆盖安装状态/版本、启动安装或更新、取消、日志以及 environment list/create/read/update/delete/use/status；不得调用任意 shell/path API。
- [x] 实现安装区：首次加载状态和版本，按安装状态显示“安装”或“更新”，运行期间禁用重复启动，提供取消，展示当前阶段、最终成功/失败/取消、稳定用户错误和按序结构化 stdout/stderr 日志。
- [x] 在 `src-tauri/src/ai_work_flow.rs` 解耦 `installed` 与可选 `version`：受管理仓库与安装流程成功时即返回已安装，版本探测缺失或无法解析不得把成功状态改为失败；前端在版本缺失时显示“已安装（版本未知）”并仍提供更新操作。
- [x] 实现环境区：加载列表和当前状态，支持创建、选择、删除、切换，并以完整 JSON 编辑器读取、编辑和保存原文；校验错误保留未保存文本，删除当前环境后立即显示 `default`。
- [x] 对异步请求实现明确的 loading、empty、running、success、error 状态，避免重复点击、过期响应覆盖当前选择和失败后错误状态丢失；页面不触发 OneSpace AI Environments API 或刷新事件。
- [x] 扩展导航、启动台可见性、更多工具 Hub、App 选中状态、invoke 封装和页面组件测试，覆盖别名、卡片、静态简介、无 README 行为、按钮禁用、取消、日志、错误、完整 JSON 往返、创建/删除/use、default 回退和 AI Environments 调用隔离。
- [x] 补充后端与前端回归测试：后端覆盖已安装但版本缺失、安装/更新成功但无法解析版本时仍为成功；前端覆盖 `installed: true` 且无 `version` 时显示“已安装（版本未知）”、更新按钮可用且不误报失败。
- [x] 在运行时注册测试中完成安装与七个环境命令的跨层契约断言，核对前端 invoke 名与实际 Tauri 注册名一一对应、各注册一次且无任意命令入口。
- [x] 执行 OneSpace 全量自动检查、应用管理仓库内 AI Work Flow 验收命令，并按首次安装、更新、网络/脚本失败、取消、重复点击、非法文件目标、default、删除当前环境和有效切换场景完成人工端到端验收。

## 验收标准

- [x] 用户可从更多工具卡片、别名目标和 App 导航稳定进入 AI Work Flow 页面，选中/返回状态正确，静态简介不存在任何 README 读取、展示或链接。
- [x] 安装区准确展示版本、阶段、运行中、成功、失败、取消和结构化日志；运行中重复操作禁用，取消和后端错误可辨识且不会启动重叠流程。
- [x] `installed` 判定不依赖 `version`；缺少或无法解析版本不会使成功的安装/更新流程失败，前端稳定显示“已安装（版本未知）”并允许更新。
- [x] 环境区可完成列表、创建、选择、删除、切换及完整 JSON 读取编辑保存；无效内容保留在编辑器中，删除当前环境后显示 `default`。
- [x] 前端 invoke 与 Tauri 注册命令完全对应，安装与环境状态跨层流转通过测试，且所有操作与 OneSpace AI Environments 数据及事件隔离。
- [x] OneSpace 的 test、lint、build、Rust 测试和 diff 检查均通过；受管理 AI Work Flow 仓库的测试、skills 校验、安装校验和环境状态命令均通过。
- [x] 人工端到端场景记录实际完整安装/更新流程，不使用 `--dry-run`、不增加确认步骤，并覆盖失败、取消和环境边界。

## 验证步骤

- [x] 运行 `npm run test`，预期导航、启动台可见性、MoreToolsHub、App、invoke 封装和 AiWorkFlowTool 测试及既有前端回归全部通过。
- [x] 运行 `npm run lint` 与 `npm run build`，预期 ESLint、TypeScript 和 Vite 构建无错误。
- [x] 运行 `cargo test --manifest-path src-tauri/Cargo.toml`，预期后端单元、命令注册与跨层契约测试全部通过。
- [x] 运行 AI Work Flow 后端与前端定向回归测试，预期 `installed: true`、`version: null` 场景保持安装/更新成功，页面显示“已安装（版本未知）”且更新入口可用。
- [x] 运行 `git diff --check`，预期无空白错误；该命令仅用于验收，不由任务工件写入阶段执行。
- [x] 在应用管理的 AI Work Flow 仓库依次运行 `npm test`、`npm run validate:skills`、`node agent-build/install.mjs validate`、`node agent-build/install.mjs env status`，预期全部退出码为 0 且 status 与页面当前环境一致。
- [x] 人工执行首次安装和已安装更新，确认固定远端与完整阶段顺序；分别注入网络/脚本失败、运行中取消和重复点击，确认停止后续阶段、状态与日志正确且无重叠进程。
- [x] 人工验证非法文件目标、无 `.environment` 的 `default`、删除当前环境及有效环境切换，确认原子性、回退和 Agents 生成符合后端契约，OneSpace AI Environments 前后数据及事件无变化。

## 范围外事项

- 不增加 README 浏览器、远端配置、任意命令输入、dry-run 或安装二次确认。
- 不迁移 `~/AiHistorys/ai-work-flow`，不迁移或同步 OneSpace AI Environments 服务商配置。
- 不改变与本功能无关的更多工具视觉、导航结构或业务模块。
