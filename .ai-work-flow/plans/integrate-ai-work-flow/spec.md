# 在 OneSpace 集成 AI Work Flow

## 规格元数据

- plan-id: `integrate-ai-work-flow`
- status: `approved`
- source_context_id: `integrate-ai-work-flow-requirements-v1`
- source_context_digest: `d469c97bc22704c226c450e0e6d4667f8b0339627a4b540a96c46ab08b04b7f0`
- revision: `3`
- revision_note: `REV-2：保留全部功能、安全和隔离要求；任务 03 以自动化、构建、受管理仓库校验及真实浏览器视口截图作为阻塞证据，真实 Tauri GUI、网络和人工端到端验证改列非阻塞延期项。`
- revision_history: `REV-1（revision 2）：将有效受管理仓库的安装状态与可选版本解耦；任务 03 的后端写入范围扩展至 src-tauri/src/ai_work_flow.rs；任务 01、02 的实施证据保持不变。REV-2（revision 3）：采纳 D-001、D-002、D-003，修订验收证据等级而不弱化产品功能语义。`

## 问题陈述

OneSpace 桌面端缺少对 AI Work Flow 的一体化入口，开发者无法在应用内安装或更新受 OneSpace 管理的 AI Work Flow 副本，也无法安全地维护其独立环境配置。集成必须保留 AI Work Flow 既有安装、环境切换和安全约束，同时不能影响 OneSpace 现有 AI Environments 服务商配置。

## 目标与成功标准

- 在“更多工具”中提供可达的 AI Work Flow 工具入口及导航。
- 首次安装和后续更新可追踪、互斥执行，并显示结果与可查看日志。
- 应用数据目录维护独立仓库副本，绝不修改 `~/AiHistorys/ai-work-flow`。
- 环境新增、查看、完整 JSON 编辑、删除和切换均有效且受校验。
- AI Work Flow 环境域与 OneSpace AI Environments 服务商配置完全隔离。
- OneSpace 测试、Lint、构建，以及 AI Work Flow 安装和环境状态验证均通过。
- 有效受管理仓库即使其 `package.json` 缺少 `version`，也必须识别为已安装；此时版本返回 `null`，安装或更新不因此失败。
- 任务 03 的阻塞验收由自动化测试、构建、受管理仓库校验和真实浏览器桌面及移动视口截图构成；测试桩不得被表述为真实 Tauri IPC、网络或文件副作用证据。

## 用户与用户故事

用户为 OneSpace 桌面端用户，主要是需要在本机安装 AI Work Flow 并管理其环境配置的开发者。

- 作为开发者，我可以从“更多工具”打开 AI Work Flow 首页，了解该内置工具并开始安装或更新。
- 作为开发者，我可以直接运行完整安装或更新，查看进行状态、最终结果和执行日志，且并发请求不会重叠执行。
- 作为开发者，我可以管理 AI Work Flow 环境的 JSON 配置，并在保存前收到配置无效提示。
- 作为开发者，我可以切换环境；删除当前环境后，系统自动回退到 `default`。
- 作为 OneSpace 用户，我的现有 AI Environments 服务商配置不会因 AI Work Flow 环境操作而改变。

## 功能需求

1. 在 OneSpace“更多工具”中新增 AI Work Flow 工具入口、展示卡片及对应导航目标。
2. 工具首页仅展示 OneSpace 内置的简短静态简介；不得读取、展示或链接本地 README。
3. 在应用数据目录维护 AI Work Flow 的独立仓库副本；不得读写或修改 `~/AiHistorys/ai-work-flow`。
4. 首次安装必须从固定地址 `https://github.com/hengboy/ai-work-flow.git` 克隆最新版本，并依次执行 `npm ci`、`node agent-build/install.mjs` 和 `node agent-build/install.mjs validate`。
5. 已安装时，更新必须拉取最新源码后重新执行完整安装流程；AI Work Flow 无独立 update 子命令。
6. 安装和更新必须直接执行完整安装流程，不执行 `--dry-run`，不要求用户变更确认或二次确认。
7. 后端仅可执行固定仓库地址以及固定的 `git`、`npm`、`node` 命令；所有文件路径均须通过受校验的路径边界。
8. 安装和更新必须提供运行状态、并发去重锁、成功或失败结果反馈及可查看执行日志。
9. 安装器可依 AI Work Flow 既有规则写入或清理其受管理目录：`~/.config/ai-work-flow`、`~/.claude`、`~/.codex` 和 `~/.config/opencode`。
10. 提供 `~/.config/ai-work-flow/environments` 中环境的新增、列表查看、完整 JSON 编辑、删除和切换能力。
11. 环境编辑界面只能提供完整 JSON 编辑器；保存前必须复用或等效执行 AI Work Flow 配置校验，无效 JSON 或无效配置不得落盘。
12. 环境名称必须符合 1 至 64 位字母、数字、点、下划线或连字符；拒绝控制字符、路径越界、符号链接和非普通文件。
13. 删除当前环境时，移除当前环境标记并回退到 `default`；缺失 `~/.config/ai-work-flow/.environment` 标记也表示 `default`。
14. 切换环境必须调用 AI Work Flow 既有切换能力，验证目标环境并生成受管理 Agents。
15. AI Work Flow 环境与 OneSpace 现有 AI Environments 服务商配置必须完全独立，不得产生读写或状态联动。
16. 安装状态必须依据受管理仓库的有效性判定，不得依赖 `package.json` 的 `version` 字段；有效仓库缺少该字段时返回 `installed=true`、`version=null`，不得返回 `version_invalid`。
17. 安装或更新在有效受管理仓库缺少 `version` 时仍可成功完成；前端必须显示“已安装（版本未知）”并保留更新操作。

## 非功能需求

- Tauri 后端命令集中注册，并遵循既有 Git 执行、异步任务、去重锁、原子目录替换和错误返回模式。
- 固定外部进程、联网仓库和受管理目录写入必须白名单化，避免任意命令或任意路径执行。
- 安装、更新、环境操作的错误必须能被前端辨识并向用户反馈；执行输出必须保留为可查看日志。
- 静态首页内容不得依赖本地文件存在性或运行时 README 读取。
- 任务 03 的后端实现允许写入 `src-tauri/src/ai_work_flow.rs`；其余路径、远端、命令和数据域隔离约束不放宽。
- 自动化 Rust 测试不得访问真实网络或真实用户目录；真实浏览器截图必须运行完整前端，并且仅替换 Tauri invoke。

## 范围

本次覆盖前端工具入口与导航、AI Work Flow 工具首页、Tauri 后端命令、独立仓库的安装更新流程、环境 CRUD 与切换、路径和文件安全边界、并发与日志状态，以及相关失败场景测试。revision 2 还覆盖任务 03 中安装状态与可选版本语义、`src-tauri/src/ai_work_flow.rs` 的合法后端写入范围，以及相关后端和前端回归测试；保留任务 01、02 已完成实施证据。revision 3 仅修订任务 03 的证据等级和完成门槛，不修改其未提交实现或上述功能语义。

仓库相关实现应优先结合以下已验证位置和模式：`src/App.tsx`、`src/components/MoreToolsHub.tsx`、`src/lib/navigation.ts`、`src/lib/moreToolPresentation.ts`、`src-tauri/src/app_runtime/run_app.rs`、`src-tauri/src/git.rs`，以及 Skills/Subagents 的远端刷新和异步任务模式。

## 接口与数据

- 前端通过 Tauri 命令查询安装状态、触发安装或更新、读取执行状态和日志，以及管理 AI Work Flow 环境。
- 后端仓库位置为应用数据目录下的独立副本；远端地址固定为 `https://github.com/hengboy/ai-work-flow.git`。
- 安装状态响应中的 `installed` 表示受管理仓库有效，`version` 为可选字段：存在时返回其值，缺失时返回 `null`。
- 环境文件位于 `~/.config/ai-work-flow/environments/<name>.json`；当前环境由 `~/.config/ai-work-flow/.environment` 标记，标记不存在即为 `default`。
- 环境命令语义对齐既有 CLI：`env list`、`env use`、`env status`、`env create`、`env delete`；不存在 `env edit`，编辑由 OneSpace 在完整 JSON 校验通过后安全写入。
- OneSpace AI Environments 服务商配置属于独立数据域，不得作为 AI Work Flow 环境数据源、镜像或同步目标。

## 失败模式

- GitHub 不可访问、克隆或拉取失败、`npm ci` 失败、安装脚本失败或 validate 失败时，安装或更新失败，保留诊断日志并返回明确错误。
- 同时发起的安装或更新请求必须被并发锁串行化或去重，不能启动重叠外部进程。
- 非白名单命令、非固定远端、越界路径、符号链接、非普通文件、非法环境名和含控制字符输入必须被拒绝。
- JSON 格式错误或未通过 AI Work Flow 配置校验时，编辑保存失败且原文件不被改动。
- 受管理仓库有效但 `package.json` 缺少 `version` 时，不得将其判定为未安装或 `version_invalid`，也不得阻断安装或更新。
- 删除当前环境后，无法保留失效当前标记；必须回退至 `default`。
- 环境切换目标不存在或校验失败时，必须拒绝切换且不破坏当前可用环境状态。

## 验收标准

以下第 1 至 13 项保持产品功能语义；真实 Tauri GUI、GitHub clone/pull、外部进程取消、真实文件副作用和人工端到端执行在当前受限环境中不作为阻塞证据，须按“延期验证与残余风险”记录，且不得由 invoke 测试桩截图冒充。

- Given 用户打开“更多工具”，When 选择 AI Work Flow，Then 可以到达工具首页并看到内置静态简介，且页面不读取或链接 README。
- Given 尚未安装，When 用户触发安装，Then 应用在独立应用数据目录从固定 GitHub 地址克隆，并执行 `npm ci`、安装脚本和 validate，全程显示状态、结果和日志。
- Given 已安装，When 用户触发更新，Then 应用拉取最新源码并重新执行完整安装流程，不使用 `--dry-run`，不要求确认。
- Given 安装或更新正在运行，When 再次触发相同操作，Then 并发锁阻止重叠执行，并向用户返回可辨识状态。
- Given 有效受管理仓库的 `package.json` 缺少 `version`，When 查询安装状态，Then 返回 `installed=true` 与 `version=null`，且不返回 `version_invalid`；When 用户触发安装或更新，Then 操作不因版本缺失失败。
- Given 前端收到 `installed=true` 与 `version=null`，When 渲染 AI Work Flow 状态，Then 显示“已安装（版本未知）”并提供更新操作。
- Given 外部命令、远端地址或文件路径不在允许集合，When 后端收到请求，Then 拒绝执行且不产生越界写入。
- Given 安装流程因网络、Git、npm、安装脚本或验证失败，When 操作结束，Then 返回失败结果并保留可查看日志。
- Given 用户创建、查看、编辑、删除或切换环境，When 输入和目标有效，Then 操作作用于 `~/.config/ai-work-flow/environments` 且状态正确。
- Given 用户提交环境 JSON，When JSON 或配置校验无效，Then 不写入文件并显示错误；When 校验有效，Then 安全写入对应普通文件。
- Given 环境名非法、路径越界、目标为符号链接或非普通文件，When 执行环境操作，Then 后端拒绝请求。
- Given 当前环境被删除，When 删除完成，Then 当前环境回退为 `default`；Given 切换有效环境，When 切换完成，Then 调用既有切换能力并生成受管理 Agents。
- Given 用户操作 AI Work Flow 环境，When 操作完成，Then OneSpace AI Environments 服务商配置未被读取、写入或改变。
- Given 任务 03 实现完成，When 运行阻塞验证，Then OneSpace `npm run test`、`npm run lint`、`npm run build`、完整 Rust 测试及 `installed=true`/`version=null` 定向回归、受管理 AI Work Flow 仓库 `npm test`、`npm run validate:skills`、`node agent-build/install.mjs validate`、`node agent-build/install.mjs env status` 和 `git diff --check` 均通过。
- Given 完整前端在真实桌面与移动浏览器视口运行，When 仅注入 Tauri invoke 测试桩，Then 截图和布局检查无空白、重叠或水平溢出，AI Work Flow 关键入口和状态可见；该证据不覆盖真实 Tauri IPC、GitHub clone/pull、外部进程取消或真实文件副作用。

## 延期验证与残余风险

- 非阻塞延期验证：真实 Tauri GUI 中的安装、更新、失败、取消和重复点击；真实 GitHub clone/pull；外部进程取消；以及真实受管理目录和环境文件副作用。
- 非阻塞延期验证还包括真实非法文件目标拒绝、`default` 回退、删除当前环境和有效环境切换的人工端到端场景。
- 残余风险：当前阻塞证据能够验证命令契约、状态、UI 渲染、布局和受控测试行为，但不能替代真实 Tauri IPC、网络、进程或文件系统副作用的现场验证。
- D-001（revision 3）：用户确认跳过当前环境无法完成的真实 Tauri GUI 验收并继续合并。
- D-002（revision 3）：保留功能要求，仅将真实 GUI 和网络副作用证据改为非阻塞延期项；自动化和截图为阻塞证据。
- D-003（revision 3）：截图使用完整前端的真实浏览器桌面及移动视口，仅替换 Tauri invoke，不使用组件测试截图。

## 兼容性与迁移

- 不迁移、不覆盖也不修改 `~/AiHistorys/ai-work-flow` 的现有仓库或内容。
- 允许 AI Work Flow 安装器按其既有规则维护受管理目录；这是经授权的兼容性边界。
- 不迁移或同步 OneSpace AI Environments 服务商配置与 AI Work Flow 环境文件。
- 已存在环境标记缺失时维持 AI Work Flow 既有语义，视为 `default`。
- 兼容真实上游 `package.json` 合法缺少 `version` 的仓库；该情形以未知版本而非未安装处理。

## 范围外事项

- 在 OneSpace 中展示、读取、解析或链接 AI Work Flow 本地 README。
- 修改 AI Work Flow 上游仓库、增加其独立 update 子命令或更改其安装器规则。
- 将 AI Work Flow 环境与 OneSpace AI Environments 服务商配置进行联动、同步或迁移。
- 扩展为用户可配置任意 Git 仓库、外部命令或任意文件系统路径。

## 假设

- 本机具备 Node.js 和 Git。
- 至少存在一个可供 AI Work Flow 安装的 AI 客户端。
- 网络可以访问固定 GitHub 仓库。
- 固定受管理仓库路径为 `~/.config/onespace/ai-work-flow/repository`。
- 真实上游 `package.json` 可以合法缺少 `version`。
- 当前 Coding 工作区和提交由后续实施流程继续承接。
- revision 2 决策 REV-1：用户确认将 `src-tauri/src/ai_work_flow.rs` 纳入任务 03，并将 `installed` 与可选 `version` 解耦，同时保留任务 01、02 实施证据。
- revision 3 决策：真实 Tauri GUI、网络、外部进程和真实文件副作用验证在当前受限环境中为非阻塞延期项；自动化、构建、受管理仓库校验和完整前端真实浏览器视口截图为阻塞证据。
- 产品决定均为 revision 1：D1 应用管理独立仓库副本；D2 直接完整安装且不做 dry-run 或二次确认；D3 仅完整 JSON 编辑并在保存前校验；D4 仅切换 AI Work Flow 环境且不联动 OneSpace AI Environments；D5 首页仅提供静态简介且无 README 入口；D6 授权联网、固定外部进程和受管理目录写入，并要求白名单、并发锁、状态和日志。

## 开放问题

N/A
