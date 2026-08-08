# 在 OneSpace 集成 AI Work Flow 实施计划

## 计划元数据

- plan-id: `integrate-ai-work-flow`
- status: `ready-for-implementation`
- source_spec: `.ai-work-flow/plans/integrate-ai-work-flow/spec.md`
- source_spec_digest: `35975371cd13544dc1e06fad0331d250096c85469aa96b8ef490ac30052179f7`
- task_mode: `split`

## 技术与代码上下文

- 前端导航与更多工具类型、别名解析位于 `src/lib/navigation.ts`；工具卡片图标与展示样式位于 `src/lib/moreToolPresentation.ts`；`src/App.tsx` 和 `src/components/MoreToolsHub.tsx` 负责更多工具入口、选择状态和内容渲染。
- Tauri 命令在 `src-tauri/src/app_runtime/run_app.rs` 集中导入和注册。新 AI Work Flow 域应使用独立 Rust 模块，例如 `src-tauri/src/ai_work_flow.rs`，不得接入 `ai_env`、`app_store` 或 OneSpace AI Environments 的服务商数据模型。
- `src-tauri/src/git.rs` 展示了应用数据目录、阻塞外部进程和互斥锁的既有模式，但其用户可配置远端与同步语义不适用于本功能。AI Work Flow 模块必须采用固定常量、自己的锁和受限命令构造器。
- 应用管理的仓库应在 `config::get_app_dir()` 返回的应用数据目录下，例如 `<app-data>/ai-work-flow/repository`；禁止探测、读取、写入或迁移 `~/AiHistorys/ai-work-flow`。
- AI Work Flow 环境唯一数据根为 `~/.config/ai-work-flow`，其中 `environments/<name>.json` 是环境文件，`.environment` 是当前环境标记；该域与 OneSpace AI Environments 的服务商存储完全隔离。

## 实施方案

1. 新建独立的后端 `ai_work_flow` 边界，集中固定仓库 URL、受限进程参数、路径验证、运行状态、日志和环境文件操作；在运行时注册其命令及单实例任务锁。
2. 在前端导航和更多工具中心加入 `ai-work-flow` 目标、工具卡和专用页面。页面只含静态简介、安装状态与版本、安装/更新及日志操作区，以及环境列表和完整 JSON 编辑器。
3. 将安装与更新统一为同一完整工作流：首次安全克隆固定远端，已安装安全拉取最新版，然后严格依次执行 `npm ci`、`node agent-build/install.mjs`、`node agent-build/install.mjs validate`。不提供 dry-run、任意命令、可配置远端或二次确认。
4. 用后端完成环境 `list/create/read/update/delete/use/status`，所有入站名称、路径、文件类型和 JSON 配置先校验，再以安全方式写入；使用既有 AI Work Flow 切换能力生成受管理 Agents。
5. 为外部进程、文件系统边界、并发状态、前端流程和域隔离补充单元、组件及集成测试，并执行 OneSpace 与已安装 AI Work Flow 的验收命令。

## 顺序执行步骤

1. 建立后端模块、状态类型和安全辅助函数：解析应用数据目录与用户配置目录，定义固定 URL `https://github.com/hengboy/ai-work-flow.git`，创建每次调用均显式传参的 `git`、`npm`、`node` 命令构造器，并注册只读状态、安装/更新、日志和环境命令。
2. 实现受管理仓库生命周期：目标不存在时克隆到同级临时目录并经校验后原子替换；目标存在时确认其为预期普通目录与 Git 工作树、固定 origin 后拉取最新源码；两种路径都继续完整安装与验证。将 stdout、stderr、阶段和最终结果写入结构化运行记录。
3. 实现环境后端：名单式命令为 `list`、`create`、`read`、`update`、`delete`、`use`、`status`；名称仅允许 1 至 64 位字母、数字、点、下划线和连字符；拒绝控制字符、路径穿越、符号链接及非普通文件。保存 JSON 前解析并调用或等效执行 AI Work Flow 配置校验，失败时不改原文件。
4. 完成环境状态语义：无 `.environment` 标记视为 `default`；删除当前环境时删除标记或写入安全的默认状态并返回 `default`；切换前确认目标存在且有效，随后调用 AI Work Flow 既有 `env use` 能力，失败不破坏原状态。
5. 接入前端导航与页面：扩展导航联合类型、别名映射、更多工具卡片和 `MoreToolsHub` 内容分支；在 `App.tsx` 保持选中导航状态一致。页面不读取、不解析且不链接本地 README。
6. 构建前端交互：首次加载获取安装与环境状态；安装和更新按钮按后端状态禁用或显示进行中；渲染版本、成功/失败和可查看结构化日志；环境页支持列表、创建、选择、删除、完整 JSON 读取与保存，并将可辨识错误展示给用户。
7. 补齐测试和验收：模拟 Git、npm、node、路径和环境文件边界，验证操作顺序、失败日志、并发去重、原子写入、当前环境回退、切换调用和 AI Environments 不受影响；验证导航、静态简介和完整 JSON 编辑行为；最后运行规定命令。

## 任务边界与依赖

本次为已确认 split 计划的原位修订，不重建任务集合。以下任务的 ID、顺序、标题和路径保持不变；task 文件是依赖、scope、实施与验收细节的权威来源。

1. order: `01`；task_id: `ai-work-flow-install-backend`；标题：实现 AI Work Flow 安装更新后端及测试；路径：`.ai-work-flow/plans/integrate-ai-work-flow/tasks/01-ai-work-flow-install-backend.md`；概要：保持原安装更新后端、安全边界、状态、日志、取消与测试职责。
2. order: `02`；task_id: `ai-work-flow-environment-backend`；标题：实现环境管理切换后端及测试；路径：`.ai-work-flow/plans/integrate-ai-work-flow/tasks/02-ai-work-flow-environment-backend.md`；概要：保持原环境 CRUD、切换、原子写入、回退与隔离测试职责。
3. order: `03`；task_id: `ai-work-flow-tool-integration`；标题：完成更多工具前端、跨层集成与端到端验收；路径：`.ai-work-flow/plans/integrate-ai-work-flow/tasks/03-ai-work-flow-tool-integration.md`；依赖：`ai-work-flow-install-backend`、`ai-work-flow-environment-backend`；概要：功能、依赖与 write scope 保持不变；阻塞证据为 OneSpace `npm run test`、`npm run lint`、`npm run build`、完整 Rust 测试、`installed=true`/`version=null` 定向回归、`git diff --check`、受管理仓库四项校验，以及仅替换 Tauri invoke 的真实桌面与移动浏览器完整前端截图布局检查；真实 Tauri GUI、网络安装更新、真实进程文件副作用和人工 E2E 为非阻塞延期验证与残余风险。

## 具体改动

- `src/lib/navigation.ts`：将 `ai-work-flow` 纳入 `MoreToolsSection`、别名映射及更多工具目标解析。
- `src/lib/moreToolPresentation.ts`：为该工具加入 Lucide 图标和现有更多工具风格的展示元数据。
- `src/App.tsx`、`src/components/MoreToolsHub.tsx`：加入入口、导航、页面选择与 AI Work Flow 专用工具内容；必要时新增局部组件或 hooks 至既有前端目录。
- `src-tauri/src/ai_work_flow.rs`（新增）：实现受限仓库生命周期、进程运行器、结构化日志与状态、环境命令、安全文件读写和校验结果映射。
- `src-tauri/src/lib.rs` 或现有模块声明位置：声明新模块；`src-tauri/src/app_runtime/run_app.rs`：导入并在 `invoke_handler!` 中仅注册该域 Tauri 命令。
- 对应的 Rust `#[cfg(test)]`、前端单元/组件测试文件：覆盖白名单、锁、状态、环境边界、页面行为和数据域隔离。

## 接口与数据流

- 前端仅通过 Tauri invoke 调用 AI Work Flow 命令：安装状态与版本查询、安装/更新启动、运行状态与日志查询，以及 `environment_list`、`environment_create`、`environment_read`、`environment_update`、`environment_delete`、`environment_use`、`environment_status`。命令名在实现中按仓库惯例确定，但不得暴露任意 shell、URL 或文件路径参数。
- 安装请求不接受仓库 URL、命令、工作目录或 dry-run 参数。后端以固定常量和参数数组调用：首次 `git clone https://github.com/hengboy/ai-work-flow.git <temporary-directory>`；更新在受管理仓库中执行固定拉取参数；随后依次 `npm ci`、`node agent-build/install.mjs`、`node agent-build/install.mjs validate`。
- 运行状态至少包含空闲、运行中、成功、失败和已取消，携带操作种类、当前阶段、时间、可辨识错误代码/消息及顺序日志条目。全局单任务锁确保任意安装或更新请求不会重叠；重复请求返回当前运行状态而不启动外部进程。
- 取消通过后端持有的运行取消信号处理：仅可取消当前受管理任务，终止其已启动的受限子进程，记录取消结果；取消、Git/npm/node 失败及 validate 失败均保留日志。
- 环境数据模型为环境名称、完整 JSON 文本/值、有效性和当前状态。JSON 保存须先解析、验证配置，再在 `~/.config/ai-work-flow/environments/<name>.json` 的经验证普通文件目标上原子替换；不得将 OneSpace AI Environments 的提供商对象转换、镜像或同步到该模型。

## 失败处理

- GitHub 不可达、clone/pull、`npm ci`、安装脚本或 validate 出错时，停止后续阶段，状态转为失败，返回稳定错误代码与用户可读消息，保留 stdout/stderr 结构化日志。
- 锁已占用时不创建新子进程；前端以运行中状态显示并禁止重复操作。取消请求针对无活动任务或已结束任务返回可辨识状态，不篡改历史结果。
- 任何非固定 URL、非白名单可执行文件/参数、应用数据目录外的仓库路径、环境根外的文件路径、符号链接和非普通文件均返回安全错误，且不执行进程或写入文件。
- 非法名称、控制字符、JSON 解析失败或配置验证失败均在写入前拒绝，保留已有环境文件。目标环境不存在或无效时拒绝切换，并保持已有环境标记与 Agents 状态。
- 删除当前环境后，无论标记原先存在与否，返回 `default` 作为当前环境；对 `default` 的缺失标记保持既有语义。环境 API 不查询、修改或发出 OneSpace AI Environments 的刷新事件。

## 测试与验证

- 任务 01、02 的既有实施边界、依赖和验收保持不变；其自动化测试继续覆盖固定 URL 与参数数组、受管理路径边界、环境名称、原子写入、配置失败不改文件、默认回退、切换前验证、并发去重、取消和失败日志。测试桩仅证明受控命令契约与状态，不得宣称覆盖真实 IPC、clone/pull、外部进程取消或真实文件副作用。
- 任务 03 的阻塞 Rust 验证包括完整 Rust 测试、Tauri 命令注册和状态转换，以及有效受管理仓库缺少 `package.json` 的 `version` 时 `installed=true`、`version=null` 的定向回归；自动化测试使用临时目录和模拟进程，不访问真实用户目录或网络。
- 任务 03 的阻塞前端验证包括导航、更多工具卡片、内置静态简介无 README 读取/链接、安装状态和日志、重复触发禁用、完整 JSON 编辑及错误、删除当前环境后的 `default` 显示和 AI Environments API 隔离；在真实桌面与移动浏览器视口运行完整前端，仅替换 Tauri invoke，完成截图与布局检查，确认页面非空白、无重叠或水平溢出，关键入口和状态可见。
- 阻塞验收命令：在 OneSpace 仓库执行 `npm run test`、`npm run lint`、`npm run build`、完整 Rust 测试及 `git diff --check`；在应用管理的 AI Work Flow 仓库执行 `npm test`、`npm run validate:skills`、`node agent-build/install.mjs validate`、`node agent-build/install.mjs env status`。

## 延期验证与残余风险

- 非阻塞延期验证保留全部产品功能目标：真实 Tauri GUI 中的安装、更新、失败、取消和重复点击，真实 GitHub clone/pull，真实外部进程取消，以及真实受管理目录和环境文件副作用。
- 非阻塞延期验证还包括真实非法文件目标拒绝、`default` 回退、删除当前环境和有效环境切换的人工端到端场景。
- 残余风险：阻塞证据验证命令契约、状态、UI 渲染、布局和受控测试行为，但不能替代真实 Tauri IPC、网络、进程或文件系统副作用的现场验证；浏览器截图中的 invoke 测试桩不构成此类证据。

## 验收标准

- 更多工具可进入 AI Work Flow 页面，页面仅显示内置静态简介，不读、不展示也不链接 README。
- 首次安装仅在应用数据目录从固定 GitHub URL clone；更新仅在该独立副本拉取最新版本；两者均依次运行 `npm ci`、安装脚本和 validate，且不使用 dry-run 或二次确认。
- 安装/更新提供版本、阶段、运行中、成功、失败、取消和可查看日志；重复请求不会产生重叠进程。
- 后端拒绝任意仓库 URL、命令、越界路径、符号链接、非普通文件、非法环境名与控制字符，且没有越界写入；仅授权安装器维护 `~/.config/ai-work-flow`、`~/.claude`、`~/.codex`、`~/.config/opencode`。
- 环境创建、列表、读取、完整 JSON 更新、删除、使用和状态均作用于 AI Work Flow 环境根；无效 JSON/配置不落盘；删除当前环境回退 `default`；有效切换调用既有能力并生成受管理 Agents。
- AI Work Flow 环境操作不会读取、写入、同步或改变 OneSpace AI Environments 服务商配置。
- 阻塞证据均成功：OneSpace `npm run test`、`npm run lint`、`npm run build`、完整 Rust 测试、`installed=true`/`version=null` 定向回归、`git diff --check`，以及受管理 AI Work Flow 仓库的 `npm test`、`npm run validate:skills`、`node agent-build/install.mjs validate`、`node agent-build/install.mjs env status`；完整前端在仅替换 Tauri invoke 的真实桌面和移动浏览器视口截图及布局检查通过。该阻塞证据不声称覆盖真实 Tauri IPC、clone/pull、外部进程取消或真实文件副作用。

## 兼容、迁移与发布

- 不迁移、覆盖、读取或修改 `~/AiHistorys/ai-work-flow`；新增功能只识别应用数据目录中的独立受管理仓库。
- 不迁移或同步 OneSpace AI Environments 服务商配置；发布前用回归测试证明两数据域和事件流隔离。
- 保持 AI Work Flow 的 `.environment` 缺失即 `default` 语义，并允许其安装器按既有规则管理授权目录。
- 无数据库迁移或历史环境格式迁移。发布前确认系统具备 Git、Node.js、网络访问及至少一个可安装 AI 客户端；缺失前置条件以明确错误和日志反馈。
