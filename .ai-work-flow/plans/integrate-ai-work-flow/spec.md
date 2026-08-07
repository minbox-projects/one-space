# 在 OneSpace 集成 AI Work Flow

## 规格元数据

- plan-id: `integrate-ai-work-flow`
- status: `approved`
- source_context_id: `integrate-ai-work-flow-requirements-v1`
- source_context_digest: `96ec7a806248ab09903b7dd2299cb21b0ee05248e9fd2d16f3a622a2d6ab07ac`

## 问题陈述

OneSpace 桌面端缺少对 AI Work Flow 的一体化入口，开发者无法在应用内安装或更新受 OneSpace 管理的 AI Work Flow 副本，也无法安全地维护其独立环境配置。集成必须保留 AI Work Flow 既有安装、环境切换和安全约束，同时不能影响 OneSpace 现有 AI Environments 服务商配置。

## 目标与成功标准

- 在“更多工具”中提供可达的 AI Work Flow 工具入口及导航。
- 首次安装和后续更新可追踪、互斥执行，并显示结果与可查看日志。
- 应用数据目录维护独立仓库副本，绝不修改 `~/AiHistorys/ai-work-flow`。
- 环境新增、查看、完整 JSON 编辑、删除和切换均有效且受校验。
- AI Work Flow 环境域与 OneSpace AI Environments 服务商配置完全隔离。
- OneSpace 测试、Lint、构建，以及 AI Work Flow 安装和环境状态验证均通过。

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

## 非功能需求

- Tauri 后端命令集中注册，并遵循既有 Git 执行、异步任务、去重锁、原子目录替换和错误返回模式。
- 固定外部进程、联网仓库和受管理目录写入必须白名单化，避免任意命令或任意路径执行。
- 安装、更新、环境操作的错误必须能被前端辨识并向用户反馈；执行输出必须保留为可查看日志。
- 静态首页内容不得依赖本地文件存在性或运行时 README 读取。

## 范围

本次覆盖前端工具入口与导航、AI Work Flow 工具首页、Tauri 后端命令、独立仓库的安装更新流程、环境 CRUD 与切换、路径和文件安全边界、并发与日志状态，以及相关失败场景测试。

仓库相关实现应优先结合以下已验证位置和模式：`src/App.tsx`、`src/components/MoreToolsHub.tsx`、`src/lib/navigation.ts`、`src/lib/moreToolPresentation.ts`、`src-tauri/src/app_runtime/run_app.rs`、`src-tauri/src/git.rs`，以及 Skills/Subagents 的远端刷新和异步任务模式。

## 接口与数据

- 前端通过 Tauri 命令查询安装状态、触发安装或更新、读取执行状态和日志，以及管理 AI Work Flow 环境。
- 后端仓库位置为应用数据目录下的独立副本；远端地址固定为 `https://github.com/hengboy/ai-work-flow.git`。
- 环境文件位于 `~/.config/ai-work-flow/environments/<name>.json`；当前环境由 `~/.config/ai-work-flow/.environment` 标记，标记不存在即为 `default`。
- 环境命令语义对齐既有 CLI：`env list`、`env use`、`env status`、`env create`、`env delete`；不存在 `env edit`，编辑由 OneSpace 在完整 JSON 校验通过后安全写入。
- OneSpace AI Environments 服务商配置属于独立数据域，不得作为 AI Work Flow 环境数据源、镜像或同步目标。

## 失败模式

- GitHub 不可访问、克隆或拉取失败、`npm ci` 失败、安装脚本失败或 validate 失败时，安装或更新失败，保留诊断日志并返回明确错误。
- 同时发起的安装或更新请求必须被并发锁串行化或去重，不能启动重叠外部进程。
- 非白名单命令、非固定远端、越界路径、符号链接、非普通文件、非法环境名和含控制字符输入必须被拒绝。
- JSON 格式错误或未通过 AI Work Flow 配置校验时，编辑保存失败且原文件不被改动。
- 删除当前环境后，无法保留失效当前标记；必须回退至 `default`。
- 环境切换目标不存在或校验失败时，必须拒绝切换且不破坏当前可用环境状态。

## 验收标准

- Given 用户打开“更多工具”，When 选择 AI Work Flow，Then 可以到达工具首页并看到内置静态简介，且页面不读取或链接 README。
- Given 尚未安装，When 用户触发安装，Then 应用在独立应用数据目录从固定 GitHub 地址克隆，并执行 `npm ci`、安装脚本和 validate，全程显示状态、结果和日志。
- Given 已安装，When 用户触发更新，Then 应用拉取最新源码并重新执行完整安装流程，不使用 `--dry-run`，不要求确认。
- Given 安装或更新正在运行，When 再次触发相同操作，Then 并发锁阻止重叠执行，并向用户返回可辨识状态。
- Given 外部命令、远端地址或文件路径不在允许集合，When 后端收到请求，Then 拒绝执行且不产生越界写入。
- Given 安装流程因网络、Git、npm、安装脚本或验证失败，When 操作结束，Then 返回失败结果并保留可查看日志。
- Given 用户创建、查看、编辑、删除或切换环境，When 输入和目标有效，Then 操作作用于 `~/.config/ai-work-flow/environments` 且状态正确。
- Given 用户提交环境 JSON，When JSON 或配置校验无效，Then 不写入文件并显示错误；When 校验有效，Then 安全写入对应普通文件。
- Given 环境名非法、路径越界、目标为符号链接或非普通文件，When 执行环境操作，Then 后端拒绝请求。
- Given 当前环境被删除，When 删除完成，Then 当前环境回退为 `default`；Given 切换有效环境，When 切换完成，Then 调用既有切换能力并生成受管理 Agents。
- Given 用户操作 AI Work Flow 环境，When 操作完成，Then OneSpace AI Environments 服务商配置未被读取、写入或改变。
- Given 实现完成，When 运行验证，Then `npm run test`、`npm run lint`、`npm run build`、AI Work Flow `npm test`、`npm run validate:skills`、`node agent-build/install.mjs validate`、`node agent-build/install.mjs env status` 和 `git diff --check` 均通过。

## 兼容性与迁移

- 不迁移、不覆盖也不修改 `~/AiHistorys/ai-work-flow` 的现有仓库或内容。
- 允许 AI Work Flow 安装器按其既有规则维护受管理目录；这是经授权的兼容性边界。
- 不迁移或同步 OneSpace AI Environments 服务商配置与 AI Work Flow 环境文件。
- 已存在环境标记缺失时维持 AI Work Flow 既有语义，视为 `default`。

## 范围外事项

- 在 OneSpace 中展示、读取、解析或链接 AI Work Flow 本地 README。
- 修改 AI Work Flow 上游仓库、增加其独立 update 子命令或更改其安装器规则。
- 将 AI Work Flow 环境与 OneSpace AI Environments 服务商配置进行联动、同步或迁移。
- 扩展为用户可配置任意 Git 仓库、外部命令或任意文件系统路径。

## 假设

- 本机具备 Node.js 和 Git。
- 至少存在一个可供 AI Work Flow 安装的 AI 客户端。
- 网络可以访问固定 GitHub 仓库。
- 产品决定均为 revision 1：D1 应用管理独立仓库副本；D2 直接完整安装且不做 dry-run 或二次确认；D3 仅完整 JSON 编辑并在保存前校验；D4 仅切换 AI Work Flow 环境且不联动 OneSpace AI Environments；D5 首页仅提供静态简介且无 README 入口；D6 授权联网、固定外部进程和受管理目录写入，并要求白名单、并发锁、状态和日志。

## 开放问题

N/A
