# 核心工作流清理与优化（计划 20261009）

本文档是计划 `20261009-core-workflows-cleanup-and-optimization`（REQ-001..007 / AC-001..007）的交付记录：交付范围、需求到验收的映射、逐项证据、移除/保留符号、数据兼容与回滚、性能基线/最终结果以及真实验证状态。实现步骤已按提交 `1da5814`、`753ad8f`、`e231ff9`、`abdeb4c`、`1b76460`、`f45b0b7`、`4c14be5` 依次落地；本文件为 Step 8 的文档收口，不改变产品行为。决策原因与完整取舍见 [核心工作流清理与优化 Note](../.ai-workflow/notes/implemented/architecture/2026-10-09-core-workflows-cleanup-and-optimization.md)。

## 1. 范围与目标

计划修复 AI 网关、AI 终端服务商、原生 AI 会话、用量统计与工具箱的入口不一致，移除确实无用的实现，把后台工作交给进程拥有，并减少重复的加载、解密、采集与数据库工作。成功以各 AC 的可测证据和操作计数衡量，而不是宣称一个抽象的全局最优架构。

七个需求与对应实现步骤/提交：

| 需求 | 验收 | 步骤 | 提交 |
| --- | --- | --- | --- |
| REQ-001 一致的会话与服务商入口 | AC-001 | Step 1 | `1da5814` |
| REQ-002 进程拥有的后台工作与真实服务生命周期 | AC-002 | Step 2 | `753ad8f` |
| REQ-003 有界移除未用代码与冗余 helper | AC-003 | Step 3 | `e231ff9` |
| REQ-004 共享快照与可见性感知刷新 | AC-004 | Step 4 | `abdeb4c` |
| REQ-005 高效用量采集与网关日志 | AC-005 | Step 5 | `1b76460` |
| REQ-006 惰性工具加载与显式前端契约 | AC-006 | Step 6 | `f45b0b7` |
| REQ-007 共享网关策略与显式配置值 | AC-007 | Step 7 | `4c14be5` |

## 2. REQ / AC 交付映射与证据

### AC-001：会话创建、恢复与服务商激活的 canonical 化（Step 1，`1da5814`）

- GUI 与内部 CLI 共享 `app_store::session_service`（`create_session`/`create_session_in_current_terminal`/`create_cli_session`/`resume_session`/`resume_session_in_current_terminal`/`resume_cli_session`）与 `provider_activation`（`activate_provider`/`use_cli_environment`/`cli_environment_listing`）；`onespace ai` 与 `onespace resume` 由生成脚本 `exec` 转发到 `__onespace_cli_create_session` / `__onespace_cli_resume_session`。
- canonical 写入门 `CanonicalStateWriteLease`（`canonical-state.lock` 的 OS 文件锁 + 进程线程互斥 + 可重入线程租约）使 GUI 与独立 CLI 进程的并发更新串行。
- 证据：`ai_env` 10；`ai_sessions` 82 通过 / 2 ignored；`app_store` 123 通过 / 2 ignored；前端 Step 1 门禁通过；`cargo check` 退出 0。真实 debug 二进制 smoke 覆盖 canonical provider `CLAUDE_CONFIG_DIR`/原工作目录/原生 ID 恢复、pending 拒绝、dev profile 生命周期与权限模式；这些 smoke 标记为 ignored（3 例真实二进制 CLI smoke）。

### AC-002：单一调度器与准确的运行状态（Step 2，`753ad8f`）

- 进程调度器 `ai_gateway/auto_refresh.rs` 在 `run_app` setup 恰好安装一次；正间隔运行一个启动批次、`0` 停放以便 rearm，保存间隔只 rearm；同模板 single-flight 让自动竞争者跳过、手动竞争者复用结果；失败快照经 `ai_gateway_template_auto_refresh_status` 与事件 `ai-gateway-template-auto-refresh-updated` 暴露。
- 监听器 `RunningServer` 以单调 `generation` 防止旧任务清理替换后的监听器；`shutdown_runtime_services` 幂等停止监听器与调度器。
- 证据：`cargo test --lib ai_gateway` 627 通过，含 `tests/auto_refresh.rs` 7 例（启动恰好一次、仅合格模板、串行、tick 跳过、自动忙碌跳过并手动 follower 复用、失败快照事件、reset/re-arm）与 `tests/runtime_lifecycle.rs` 2 例（故障/重启、旧任务不能清理替换监听器）；`app_runtime` 14 通过 / 3 ignored；前端 Step 2 门禁 19 文件 513 通过。

### AC-003：安全退役与保留历史行为（Step 3，`e231ff9`）

- 通过最终静态/动态/原生/CLI 调用方审计后移除 Rust 孤儿闭包、十个未注册命令与其前端封装、三个前端 helper 与 `historyStore.ts` 模块，并清理 npm 依赖。
- 短链历史简化为单次原始读取与解析（`dedupeHistoryRecords`），保留全有或全无恢复、50 条上限前的按时间倒序与可观察错误结果。
- 证据：`ai_env` 10；`ai_sessions` 82 / 2 ignored；`app_store` 123 / 2 ignored；`claude_profiles` 34；前端 Step 3 门禁 9 文件 101 通过；`shortLinkHistory` 与 `ShortLinkTool` 48 个未修改测试通过；`npm run build` 退出 0；`package-lock.json` 用 `--package-lock-only` 重生成（仅锁元数据归一化到 `0.1.44`，三个应用版本文件不变）。

### AC-004：一致的快照与隐藏页面不工作（Step 4，`abdeb4c`）

- `src/lib/runtimeStatus.ts` 为 gateway/router/ssh-tunnels/file-sharing 提供每 WebView 共享 store（有效快照零查询、至多一次合并回退、单飞 + 序列守卫），`useVisibleInterval`/`useTauriEvent(respectVisibility)` 按 document + 原生窗口合并可见性暂停，App 持有每服务一个事件桥。
- 后端按文件身份缓存 SSH 解码状态（含 `.local_key` 身份）与会话快照，成功写入刷新、失效即清且绝不回退旧值。
- 证据：`cargo test --lib ssh_tunnels` 68 通过（5 个缓存测试）；`workspaces` 2 通过（1 个共享快照测试）；`app_store` 127 通过 / 2 ignored（4 个缓存测试）；前端 Step 4 门禁 16 文件 266 通过（含新增 `runtimeStatus.test.ts` 与三个先前 RED）；`npm run build` 退出 0；聚焦缓存子集 44 通过。

### AC-005：正确聚合与更少重复工作（Step 5，`1b76460`）

- `ai_sessions/usage_cache.rs` 两层采集缓存（L1 整份 `ToolScan` 30s TTL、按来源根键控并合并；L2 单来源解析按 path+len+mtime）；`history.rs` 的 `HistoryFileCache` 复用 claude/codex/antigravity 的 per-file 解析。
- 网关日志经 `ai_gateway/usage_store.rs` 按路径复用写连接，`append_batch_with_accounting` 单请求一个显式事务（提交后才清理保留、才递增计数），写入在 `spawn_blocking` 上执行且失败隔离。
- 证据：`ai_sessions` 83 / 3 ignored；`app_store` 127 / 2 ignored；`ai_gateway` 631 / 2 ignored（4 个新原子性测试 + 1 ignored 计数器测试）；前端 AiUsageStats 23 通过；`npm run build` 退出 0；测量 harness 最终校验退出 0。

### AC-006：推迟加载且导航不变（Step 6，`f45b0b7`）

- 描述符用 `loadComponent`（动态 import，默认导出组件）取代急切 `component`，`ToolboxNavigationProps` 表达 JTT/SSH 导航；`MoreToolsHub` 仅在打开时解析并具备 idle/loading/ready/error、可访问加载态、可重试错误与 `AppErrorBoundary`。
- `src/main.tsx` 在加载任何应用模块前经 `resolveEntryKind` 动态选择 `QuickAiApp` 或 `App`；`QuickAiApp` 只加载 quick UI + ThemeProvider。
- 证据：真实 `npm run build` 的产物图——入口 `index-*.js` 312.80 kB（gzip 95.91）、`App-*.js` 1217.40 kB 动态、`QuickAiApp-*.js` 5.26 kB（gzip 2.16，仅静态导入 aiSessions/i18n/icons/tauri/ThemeProvider/vendor）；逐工具 chunk 已产出（Bookmarks 10.03、FileSharingTool 10.05、JsonParserTool 3.03、JttDataParserTool 44.79、Md5EncryptionTool 7.18、ProtocolRouterTool 23.52、RandomPasswordTool 9.58、ShortLinkTool 17.86、SshServers 17.91、SshTunnels 70.48、AiWorkflowModelSwitcher 34.09 kB）；前端 Step 6 门禁 11 文件 240 通过。

### AC-007：等价转发与稳定的显式值（Step 7，`4c14be5`）

- 删除 `storage::query_model_reasoning_efforts` 与 `storage::normalize_template_prices_and_efforts` 及其读/写调用，普通读写不再猜测；显式存储值、`normalize_stored_config`、`scope_model_prices`、迁移版本门控与模板显式规则保留。
- 决策核心抽到 `ai_gateway/attempt_policy.rs`（候选/key/重试/探测决策），传输专属首字节边界、SSE 回放与取消留在 `runtime_http.rs`。
- 证据：新增 `tests/attempt_policy.rs` 15 个行为测试；未修改的 `tests/templates.rs::ac007_unrelated_save_keeps_explicit_values_and_does_not_guess` 为 GREEN；`ai_gateway` 647 通过 / 2 ignored；`tools/check-ai-gateway-redaction.sh` 退出 0（凭据字面量=0）；前端 AiGateway + lib 17 文件 495 通过；`npm run build` 退出 0。

## 3. 移除 / 保留符号账本

### 移除（Step 3 及后续）

- Rust 孤儿闭包：`save_ai_providers`/`save_ai_providers_internal`、`save_ai_session`/`delete_ai_session`、`create_native_session`/`launch_native_session`/`launch_native_session_for_create`、no-op 的 `restore_missing_service_provider_api_keys_from_legacy` 及其惰性调用、`runtime_profile_exists`/`materialize_strict_profile`/`cleanup_stale_runtime_profiles`、`ServiceProviderInput`、`claude_profiles::claude_profile_dir`/`get_claude_config_dir`、`AppSnapshot`、`build_projection_diff`。
- 十个退役命令及其封装：`check_config_conflicts`（连同整个 `config_conflict.rs` 模块）、`apply_ai_environment_force`、`storage_get_snapshot`、`sessions_launch_with_prompt`、`sessions_usage_stats`（含仅被其消费的 `build_sessions_usage_stats`/`SessionUsageStatsResponse`）、`projection_dry_run`、`migration_status`、`migration_run`、`service_providers_set_inactive`、`service_providers_set_env_managed`。
- 前端 helper / 模块：`readLocalJson`、`isLauncherToolVisible`、`confirmSensitiveAction`/`SensitiveActionKind`/`SENSITIVE_ACTION_PRESETS`、`src/toolbox/historyStore.ts`。
- 隐式猜测（Step 7）：`storage::query_model_reasoning_efforts`、`storage::normalize_template_prices_and_efforts` 及两处调用。
- npm 依赖：`ssh2`、`ssh2-promise`、`base-64`、`filesize`、`@types/base-64`、`@types/filesize`、`@types/uuid`。

### 显式保留的活跃符号

- `ai_sessions/terminal.rs` 的 `launch_native_session_with_options` 与 `launch_native_session_for_create_with_options`（`*_with_options` 变体仍在使用）。
- `runtime_profiles::runtime_env_for_profile`（严格模式会话启动仍读取并复用既有 profile）。
- `app_store::get_claude_config_dir`（活跃命令）与 `claude_profiles::get_claude_profiles_dir`。
- `StorageEngine::atomic_write`（唯一原子写入实现）。
- 永久版本门控迁移读取器与 `migration.rs`。
- `scope_model_prices`（写路径价格裁剪）与全部保留的 schema 字段、旧值与显式存储值（含空 effort 列表、显式/零价格、已存本地模型）。
- 模板同步、退役删除、忽略模型、价格与默认模型清理、best-effort 终端重同步的显式规则。

## 4. 数据兼容与回滚

- 数据兼容：本计划未引入新的持久化 schema 迁移；`GATEWAY_CONFIG_SCHEMA_VERSION`、用量库 `PRAGMA user_version`、模板状态与全部字段序列化保持既有版本门控。会话与 provider 仍写入各自 canonical 加密文件；缓存（SSH 解码状态、会话快照、用量来源、网关写连接）均为进程内存，重启即重建，绝不作为唯一数据源。
- 回滚：每个合并步骤可独立回退——回退该连贯提交集与生成的 CLI 脚本即可，同时保留 canonical 加密数据、密钥、消息与日志数据库。回滚绝不把记录迁回旧 CLI JSON 文件，也不删除已存储的显式值。内存缓存与调度在重启后重建。模板退役在现行契约下仍是有意破坏性的，本计划不承诺回滚能恢复已退役的行。
- 验证：用覆盖受影响边界的入口、迁移、计费与 HTTP 测试验证恢复。

## 5. 性能基线与最终结果

测量由 `tools/measure-core-workflows.mjs`（仅用 node 内置模块）运行 `core_workflows_perf` ignored 用例完成：解析 `CWF_METRIC dataset=... phase=... wall_ms=... key=...`，采集 cold + 5 次 warm，记录环境与数据集维度，并区分 harness 计时与原始 wall；缺失时如实 `not_measured`（不用阈值断言）。

环境：Apple M1 Max，rustc 1.93.1；前后使用相同合成数据集（用量 1000 会话/工具 × 20 消息 × 4 工具；网关 10000 请求 × 3 次尝试；SSH 100 条记录）。

| dataset | 指标 | 基线 | 最终 |
| --- | --- | --- | --- |
| usage | warm `source_reads` | 3001（每趟） | 3001（持平） |
| usage | warm `cache_hits` | 0 | 每次通过 +4 |
| usage | warm wall | ~207 ms | ~25 ms |
| gateway | cold `db_opens` | 10000 | 1 |
| gateway | cold wall | ~18.2 s | ~4.4 s |
| gateway | warm wall | ~14–18 s | ~4.4 s |
| gateway | 每请求 transactions / rows / batches | 不变 | 不变 |
| ssh | cold wall | 52 ms | 52 ms |
| ssh | warm wall | 未测 | ~0.13 ms |

两次独立 final harness 运行结果一致。限制：行为测试不含绝对计时阈值；负载与机器状态会造成波动；OpenCode 历史未按文件备忘录化；被替换的用量来源在 TTL 到期或显式刷新前保持过期；Windows 未测试。

## 6. 验证状态与已知偏差

最终树上的完整检查：

| 检查 | 结果 |
| --- | --- |
| `npm test` | 95 文件 / 1479 通过，退出 0（重跑） |
| `npm run lint` | 0 error / 409 warning |
| `npm run build` | 退出 0 |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 1211 通过 / 0 失败 / 11 ignored（重跑） |
| `tools/check-ai-gateway-redaction.sh` | PASS（凭据字面量=0） |
| `tools/check-cli-matrix.test.sh` | PASS（stub 测试） |
| `ai-workflow context validate --all` / `ai-workflow notes validate` | valid |
| `tools/measure-core-workflows.mjs` | 两次 final 运行一致 |

已如实记录的首次运行 flakiness（均在隔离/重跑中通过，未修改产品代码）：

- Vitest 卸载后未处理异常 `window is not defined`（`App.headerToolsStatus`）。
- `retry_policy_zero_retry_after_ms_retries_immediately`。
- `mcp_runtime legacy_sse_transport`（既有问题，属本计划范围之外）。

未执行 / 阻塞：原生 macOS GUI smoke（工具箱别名、快速窗口、草稿隐藏/显示）因无 GUI runner 而 BLOCKED/未执行；Windows 未测试。
