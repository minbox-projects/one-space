# Agent Note: Core Workflows Cleanup and Optimization

Status: implemented

[English](2026-10-09-core-workflows-cleanup-and-optimization.md) | 中文

## Problem

GUI 的会话创建与恢复，以及 `onespace ai` / `onespace resume` CLI 入口，已各自演化为不同实现。安装的 CLI 会改写会话名（空格与点号转为下划线）、自行拼装启动命令、写明文会话 JSON，并伪造原生会话 ID；GUI 则走另一套新终端适配器。`resume` 分支在 Shell 脚本里硬编码各工具恢复命令，忽略共享的 provider 与 runtime 环境。服务商激活与 `onespace env use` 同样分叉，而 OpenCode 的活动服务商被表示为单个槽位，而非其真实的多活动集合。本记录描述计划 `20261009-core-workflows-cleanup-and-optimization` 已交付的事实；当前正文覆盖前两步，该计划的后续步骤不在此描述。

## Decision

- 让 `src-tauri/src/app_store/session_service.rs` 成为 GUI 与内部 CLI 共享、不依赖 AppHandle 的 canonical 会话入口。`create_session<F, T>(SessionInput, launch) -> Result<(ApiOk<SessionRecord>, T), ApiErr>` 接收一个适配器，由它返回已发现的真实 native ID 与自身输出。`create_session_in_current_terminal(SessionInput, &[String]) -> Result<(ApiOk<SessionRecord>, ExitStatus), ApiErr>` 与 `create_cli_session(&[String]) -> Result<(ApiOk<SessionRecord>, ExitStatus), ApiErr>` 在其上构建。`create_cli_session` 从转发参数读取工具、可选显示名与原生 argv，使用调用终端的工作目录，并消费成对出现的可选 `--permission-mode <mode>`，因此它绝不会成为显示名或到达原生工具。
- 在 `src-tauri/src/ai_sessions/terminal.rs` 增加 `LaunchOptions::launch_create_in_current_terminal`。它继承当前终端的 stdin、stdout 与 stderr，保留已成功启动子进程的非零退出码，并且仅在 spawn 本身失败时返回 `Err`。
- 新建记录按用户输入原样保留名称，初始为 `pending_bind`，仅在历史发现真实原生会话 ID 后才变为活动；显示名绝不代替该 ID。workspace MCP 准备在发布 pending 记录之前执行。spawn 失败会移除记录；已启动但之后以非零状态退出的子进程会保留。
- 会话与服务商保持单一 canonical 写者。并发创建仍按 `(tool, working_dir, runtime_mode, runtime_profile_id, preset_id)` 作用域经 `acquire_session_create_lock`/`release_session_create_lock` 去重。跨进程串行由 `src-tauri/src/app_store/storage_engine.rs` 提供：`CanonicalStateWriteLease` 持有当前 profile 下 `canonical-state.lock` sidecar 的持久独占 OS 文件锁、进程级线程互斥 `SESSIONS_STATE_WRITE_LOCK` 以及锁定路径，线程本地 `CANONICAL_STATE_WRITE_LEASE`（`RefCell` 中的 `Weak` 升级为 `Rc` 租约）使同线程嵌套获取可重入并阻塞其他线程与进程。Unix 取独占 `flock` 并重试 `EINTR`；Windows 对 sidecar 单字节取 `LockFile` 并重试 `ERROR_LOCK_VIOLATION`。`lock_sessions_state_write`（`types/api_session_types.rs`）与 `lock_service_provider_operation`（`service_provider_commands.rs`）都路由到 `lock_canonical_state_write`，`providers_storage` 的 loaders、migration 与写入加入同一门，事务中切换 profile 会被拒绝。`StorageEngine::atomic_write` 经唯一 `tmp-<pid>-<counter>`（`create_new` 打开）写入，再 `write`、`sync_all`、`rename`，失败时移除临时文件。公共 API 不变。
- 同样共享恢复入口：`resume_session<F, T>(session_id, permission_mode, initial_prompt, launch) -> Result<(ApiOk<SessionRecord>, T), ApiErr>`、`resume_session_in_current_terminal(session_id, permission_mode) -> Result<(ApiOk<SessionRecord>, ExitStatus), ApiErr>` 与 `resume_cli_session(&[String]) -> Result<(ApiOk<SessionRecord>, ExitStatus), ApiErr>` 接受 `resume <id> [--permission-mode default|full_access]`，先按 `tool_session_id`、再按 canonical `id` 查找记录。`LaunchOptions::launch_resume_in_current_terminal` 复用 `build_resume_command`，合并 provider/runtime/权限环境，继承当前终端 I/O，并保留已启动子进程的非零退出码。`resume_session` 对 pending/unbound 或空原生 ID 先按工作目录与时间窗从 native history 解析，仍失败则以 `SESSION_ID_MISSING` 拒绝，对同一工具的其他记录做 `SESSION_ID_CONFLICT` 检查，spawn 失败时记录保持不变。
- 让 `src-tauri/src/app_store/provider_activation.rs` 成为共享、不依赖 AppHandle 的服务商入口。`activate_provider(tool, provider_id)`、`use_cli_environment(tool, target)` 与 `cli_environment_listing()` 同时支撑 GUI 激活与 `onespace env use`/`env list`。
- OpenCode 激活追加到 `active_opencode` 集合（去重、允许多项），其他工具保持单一活动服务商。`onespace env use` 仍不做任何 CLI 投影，`env list` 只读取 `active_opencode`（含空集），绝不读取遗留的 OpenCode 单槽。
- 生成的 `onespace` 脚本改为转发到后端，而不再自行解析命令。其 `ai` 分支 `exec` `"$APP_BIN" __onespace_cli_create_session "$@"`，`resume` 分支 `exec` `"$APP_BIN" __onespace_cli_resume_session "$@"`；帮助与用法文本为 `ai` 和 `resume` 都记录 `--permission-mode default|full_access`。创建仍经 `build_create_command`/`configured_create_command` 从 `config::ai_model_launch_commands`（Settings -> AI Terminal 的启动命令模板）解析启动命令。
- 在一处解析权限模式：`validate_and_resolve_permission_mode` 在工具配置为 `default` 时拒绝 `full_access`；在工具配置为 `full_access` 时要求显式选择，否则返回 `PERMISSION_CONFIRMATION_REQUIRED` 且不启动、不改动状态；显式 `default` 以非提权方式运行。CLI 标志在创建与恢复时都解析并校验，绝不弱化配置边界。
- provider 配置目录完全按后端解析结果传递。此前自行创建的 `resolve_path_with_home_spelling` helper 已移除，测试改用 `canonicalize` 校验返回配置目录的身份，而不再做 home 拼写改写。
- 统一前端会话与服务商契约。`src/lib/aiSessions.ts` 承载 `AiSession`、`AiSessionListItem`、`SessionInput`、`AiModelLaunchCommands`、`AiSessionStorageConfig` 以及 `getAiSessionStorageConfig`、`sessionsList`、`sessionsCreate`、`sessionsUpdate`、`sessionsLaunch`、`sessionsDelete`、`sessionsSetFavorite`、`checkCliInstalled`、`installCli`、`hideQuickAiWindow` 与 `resizeQuickAiWindow` 封装。`src/lib/serviceProviders.ts` 承载 provider DTO、`CliTool`、`getActiveProviderIds` 与 `serviceProviders*` 命令封装。`AiSessions`、`AiSessionsList`、`QuickAiSessionBar` 与 `AiEnvironments` 使用这些类型化封装，并继续为既有导入再导出既有 DTO 类型。New Session 的 provider 摘要按所选工具列出每一个活动 provider，而非只显示单槽，OpenCode 空集合时不显示任何条目。

- Step 2 — 由进程拥有自动模板刷新：`src-tauri/src/ai_gateway/auto_refresh.rs` 为整个应用进程拥有计划。`start_scheduler` 由 `app_runtime/run_app.rs` 在网关 autostart 后恰好安装一次；安装时读取持久化间隔：正值武装计划并运行恰好一个启动批次，`0` 安装停放的控制环，后续 `request_rearm` 可在不运行批次的情况下武装它。`ai_gateway_template_auto_refresh_save` 只重新武装、绝不立即运行批次。同一时刻只运行一个批次（批次期间到达的 tick 被跳过），按模板顺序串行迭代 `models_url` 经 trim 后非空的模板。间隔契约不变：`DEFAULT 60`、`MIN 10`、`MAX 1440`。
- Step 2 — 暴露每模板失败并共享同模板工作：每模板失败只存进程内存，失败时写入、成功时清除、重启即空。它经 `ai_gateway_template_auto_refresh_status() -> TemplateAutoRefreshStatus { failures: [{ template_id, reason }] }` 与事件 `AI_GATEWAY_TEMPLATE_AUTO_REFRESH_UPDATED_EVENT = "ai-gateway-template-auto-refresh-updated"` 暴露，仅在失败集合真正变化时发出；自动失败绝不弹 toast。手动 `ai_gateway_sync_provider_template` 与自动批次共享 `start_template_op` / `TemplateOpOwner` / `TemplateOpFollower`：自动竞争者在忙时跳过，手动竞争者等待并复用 owner 的在途结果而不重复拉取，且仅在条目仍指向完成中的操作时才移除。
- Step 2 — 由后端产生 additions-only 消息：在配置写入成立的成功同步之后，`templates::template_sync_addition_notices` 只统计同一服务商同步前不存在的 `upstream_model` 映射，`commands` 以事件时语言为每个发生变化的模板恰好记录一条 `template_sync` info 消息。退役通知、忽略模型处理、退役价格与默认模型清理以及 best-effort 终端刷新保持既有顺序与语义。
- Step 2 — 让监听器状态绑定任务身份并干净停止：`RunningServer` 携带单调 `generation`；`run_server` 仅在槽位仍持有同一 generation 时退休槽位并发布 `running=false`（既有 `ai-gateway-status-update` 事件），因此出错的旧任务绝不清除替换后的监听器，同一端口可重启。`shutdown_runtime_services` 幂等地停止网关监听器与模板计划，并由托盘退出、`quit_app` 与 `RunEvent::Exit` 调用。
- Step 2 — 将前端降级为只读/订阅适配器：挂载在 `src/App.tsx` 的 `useTemplateAutoRefresh` 现在只在挂载时执行一次 `ai_gateway_template_auto_refresh_status` 读取，并订阅 `ai-gateway-template-auto-refresh-updated` 一次。它不保留计时器、不发起同步调用、没有 in-flight 注册表、不构建消息，`useTemplateAutoRefreshFailures` 保持其 map 形状。`src/lib/aiGateway.ts` 新增 `aiGatewayTemplateAutoRefreshStatus`、`TemplateAutoRefreshFailure` / `TemplateAutoRefreshStatus` 类型与事件字面量；AI Gateway 手动同步保留本地 spinner 并依赖后端 guard 与快照。

## Alternatives considered

- 保留 Shell 脚本写明文会话 JSON，只修补名称或 ID。拒绝原因：它无法与 GUI 共享 canonical 注册、写者协调、pending 绑定或 spawn 失败回滚。
- 保留 Shell 脚本里硬编码的各工具恢复命令。拒绝原因：恢复将无法与 GUI 共享 native ID 解析、provider 与 runtime 环境、权限处理或写者协调。
- 让 `onespace ai` 走 GUI 的新终端适配器。拒绝原因：CLI 运行在调用者终端中，必须继承其 I/O 与退出状态，而不是打开新终端窗口。
- 把非零退出的子进程视为创建失败并删除记录。拒绝原因：已启动但之后以非零状态退出的进程是必须保留的合法结果；只有 spawn 失败才使记录失效。
- 允许 `--permission-mode full_access` 提升配置为 `default` 的工具。拒绝原因：那样任何调用方都能绕过配置的权限边界。
- 把 OpenCode 激活表示为替换单个活动服务商。拒绝原因：OpenCode 支持多个活动服务商，GUI 与 CLI 必须呈现同一集合。
- 让 `env use` 像 `Apply to CLI` 一样投影 CLI 配置。拒绝原因：`env use` 的文档语义是无投影的绑定更新，投影是另一个显式动作。
- Step 2 — 保留前端调度器，只为隐藏窗口增加后端循环。拒绝原因：两个 owner 必须协调计时、失败状态与通知；单一进程调度器消除重复所有权，而不是协调它。
- Step 2 — 让自动竞争者等待忙碌的同模板手动同步。拒绝原因：定时批次不应阻塞在操作者工作上；自动竞争者跳过，而手动竞争者复用进行中的结果。

## Consequences

- GUI 与 `onespace ai` 现在通过同一条 canonical 路径注册、准备与绑定会话，记录可保持 `pending_bind` 直到发现真实 native ID。
- `onespace ai` 保留传入名称、使用调用目录与当前终端、原样转发额外参数，并返回子进程退出状态；`onespace resume` 解析 GUI 所用的同一条 canonical 记录、原生 ID、工作目录、环境与权限模式。
- 并发运行的 GUI 与单独启动的 `onespace ai` 进程在 profile 级 `canonical-state.lock` 上串行，因此一个进程完成的激活或注册不会被另一个进程的重叠更新丢失。该门按线程可重入，并让嵌套的 loader 与 save 加入当前租约，而不是以相反顺序获取 provider/session 锁。
- 已验证证据：provider 激活回归曾两次失败（两个 worker 都报告成功却丢失一个更新），加入该门后通过（`concurrent_gui_and_cli_opencode_activation_keep_both_changes`）。修正后的 `concurrent_independent_registrations_preserve_sessions_and_rollback` 使用完全迁移的 fixture：两个独立 worker 的 native stub 都在任一方释放前启动并保持，同时一个失败的 spawn 在另一个仍存活的成功 native 子进程持有期间完成回滚；随后公开列表显示三条成功的 pending 记录、没有失败记录，且加密状态完整。
- 真实 debug 二进制 smoke 均为 GREEN：`onespace resume` 使用 canonical provider `CLAUDE_CONFIG_DIR`（canonical 路径身份）、原始工作目录与已发现的 native ID，并拒绝 pending/unbound 记录；dev profile 生命周期只写 `$HOME/.config/onespace-dev`，release 的 `$HOME/.config/onespace` sentinel 保持不变，stub 写入真实的 `$HOME/.claude/history.jsonl`，create 绑定 `native-claude-777`，resume 使用该 ID；权限 smoke 显示配置为 `full_access` 的工具在无显式选择时以 `PERMISSION_CONFIRMATION_REQUIRED` 拒绝且不启动、不改变状态，`--permission-mode default` 以非提权方式成功，`--permission-mode full_access` 携带 `--dangerously-skip-permissions`，配置为 `default` 的普通运行保持非提权。
- OpenCode 在 GUI 与 CLI 列表中都可持有多个活动服务商；空集合保持为空，不回退到遗留单槽，而其他工具仍为单活动。
- provider 与会话 DTO 及命令封装现位于 `src/lib/`，因此被触及的组件调用类型化封装，而非动态 `any` 解释。
- 本已交付步骤未完整或部分取代任何活动记录。[OpenCode Session Storage Compatibility](../bug-fix/2026-09-20-opencode-session-storage-compatibility.md) 仍约束历史来源选择，该行为未变；[AI Terminal Session Workflow Presets and Runs Are Removed Full-Stack](../simplification/2026-09-29-ai-session-workflow-presets-removal.md) 仍约束更早的预设移除。
- Step 2 部分取代：[Provider Templates Refresh Automatically on a Persisted Interval](../feature/2026-09-23-template-auto-refresh.md)、[Template Auto Refresh Runs One Batch at App Start](../bug-fix/2026-09-29-template-autorefresh-startup-batch.md) 与 [Automatic Template Sync Notifies the Message Center on Real Mapping Changes](../feature/2026-09-24-ai-gateway-template-auto-sync-notification.md) 保留并交叉链接。它们的间隔、启动批次与 additions-only 决策继续有效，而本步骤替换了前端调度器所有权、前端消息生成以及通知记录中已被拒绝的后端替代方案。没有记录被完整取代，因此没有任何记录被归档。
- Step 2 已验证证据：`cargo test --lib ai_gateway` 通过 627，其中 `tests/auto_refresh.rs`（启动恰好一次、仅合格模板、串行、tick 跳过、自动忙碌跳过并手动 follower 复用、失败快照事件、reset/re-arm）与 `tests/runtime_lifecycle.rs`（故障状态/重启，旧任务不能清理替换后的监听器）；`app_runtime` 通过 14、3 ignored；前端 Step 2 门禁通过 19 文件 513 测试，含 `AiGateway` 67、`ProviderTemplateSection` 18、hook 适配器 4 与 `App.runtimeOwnership` 1；`npm run build` 退出 0。
- Step 2 诚实限制：生产 `start_scheduler` 的 Wry 路径仅通过受控执行器的测试接缝覆盖；Windows 分支未在本 macOS 主机上做运行时测试；本次未重跑 release-profile 运行时 smoke。

- 诚实的限制：release-profile 的权限行为未独立测试；`--permission-mode` 的缺值/非法值路径已实现但只有单测覆盖；显示名无法通过真实二进制 smoke 观测，由共享服务特征化覆盖；Windows `LockFile` 分支未在本次 macOS-only 验证中做运行时测试。该计划后续步骤尚未开始，且刻意不在此描述。
