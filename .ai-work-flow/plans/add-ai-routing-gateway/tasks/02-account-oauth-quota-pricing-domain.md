# 02 - 账号池、OAuth、额度、模型与价格领域

- task_id: `task-02`
- order: `02`
- blocked_by: `task-01`
- source_plan: `../plan.md`
- source_plan_digest: `92f85a7f07acc328e48edf775eae5bfb751f58861b7c1b93de18fb68ed5fd822`
- write_scope: `src-tauri/src/ai_routing_gateway/ 内账号、组、标签、凭据、模型目录、映射、oauth、usage、pricing 领域模块及本阶段领域 DTO、错误、事件载荷`

## Outcome

后端能够完整管理账号池及其模型能力，通过受控官方 Codex OAuth 或第三方 API Key 建立安全凭据，并以动态额度和价格快照提供可测试的路由输入。

## Implementation Checklist

- [x] 负责 `ai_routing_gateway` 内账号、组、标签、凭据、模型目录、映射、OAuth、额度刷新和价格领域实现及其存储事务。
- [x] 负责本阶段领域 DTO、错误和事件载荷定义；跨层公共类型按领域拆分，后续任务不得重新定义同义结构。
- [x] OAuth 仅实现固定官方端点、client ID、scope 和刷新语义，包括 PKCE loopback、手动完整回调 URL、Device Code。
- [x] 实现稳定账号 ID upsert、默认组规则、删除迁移、永久删除确认令牌校验所需的后端领域能力。
- [x] 实现 OAuth 动态额度窗口、刷新合并与退避、阈值判定、首页统计口径及模型价格优先级。

## Acceptance Criteria

- [x] 每个账号恰属一个组且可关联多个标签；默认组唯一且不可删除，删除非空组在同一事务迁移账号。
- [x] 永久删除账号会删除凭据、额度和模型映射，但不会删除请求历史与聚合快照。
- [x] 第三方账号只接受允许的两种 OpenAI-compatible 上游协议，API Key 离开调用边界后立即加密且读取接口不返回明文。
- [x] 公开模型与账号映射严格控制可用模型，未映射模型不可透传。
- [x] PKCE、state、固定 scope、loopback、手动回调和 Device Code 状态机均严格校验，临时材料只驻留内存并在所有终态清理。
- [x] 同一稳定 OAuth 账号重新授权时，凭据和元数据原子替换；并发额度刷新合并，失败执行有上限退避。
- [x] 动态额度支持全局、模型、端点、能力和未知窗口；0/10/100 阈值、继承/覆盖、过期降级及自动恢复符合计划。
- [x] 官方价格和第三方覆盖具备明确优先级；缺少价格或用量时费用保持不可计算。
- [x] 本机 mock 测试覆盖 OAuth 三路径、Device Code 全状态、刷新语义、账号事务、额度窗口及价格快照，不访问公网。

## Verification Steps

- [x] 执行本任务 Acceptance Criteria 对应的账号、OAuth、额度、模型与价格测试并确认全部通过。

## Verification Evidence

OAuth 发布门禁：截至实施时，OpenAI 未公开允许第三方桌面应用使用的 Codex OAuth client 注册、固定 client ID/scope 和授权契约。生产入口固定返回 `oauth_release_blocked`，未复制或逆向官方客户端材料；PKCE、loopback/手动回调和 Device Code 使用本机 fixture 验证状态机，符合父计划“不可用则停止发布该能力”的要求。

| Acceptance | Evidence |
|---|---|
| 账号、组与标签 | `accounts::tests::group_delete_moves_accounts_atomically_and_default_is_immutable`、`account_supports_multiple_tags_and_unmapped_models_never_resolve`、`account_updates_group_sort_note_enable_threshold_and_health_without_exposing_secret` 通过。 |
| 永久删除及历史保留 | `accounts::tests::permanent_delete_requires_one_time_confirmation_and_preserves_history_snapshots` 通过；凭据、额度、映射为 0，request log 与 daily aggregate 保留。 |
| 第三方协议与凭据保护 | `accounts::tests::api_key_is_encrypted_before_storage_and_read_dto_never_contains_plaintext` 通过；只接受类型化 `responses`/`chat_completions`，无读取 DTO 包含明文。 |
| 模型映射门禁 | `accounts::tests::account_supports_multiple_tags_and_unmapped_models_never_resolve` 通过；未映射和禁用映射均返回 `None`。 |
| OAuth 三路径与清理 | `oauth::tests::{pkce_loopback_and_manual_full_callback_share_strict_memory_session,callback_rejects_state_error_and_non_loopback_then_cleans_terminal_session,loopback_listener_failure_preserves_manual_callback_fallback,device_code_honors_interval_slow_down_and_all_terminal_states}` 通过；生产门禁由 `production_oauth_is_release_blocked_without_public_contract` 验证。 |
| 稳定账号与刷新 | `accounts::tests::oauth_reauthorization_keeps_stable_account_and_atomically_replaces_tokens`、`quota::tests::concurrent_refreshes_coalesce_and_failures_back_off_with_cap` 通过。 |
| 动态额度 | `quota::tests::{oauth_dynamic_windows_persist_and_api_key_accounts_are_rejected,dynamic_scopes_and_unknown_window_rules_apply_to_matching_requests_only,threshold_boundaries_inheritance_stale_and_recovery_are_exact,homepage_denominator_counts_accounts_not_windows}` 通过。 |
| 价格与不可计算费用 | `pricing::tests::{account_override_wins_and_snapshot_does_not_change_retroactively,missing_price_or_usage_is_not_zero_and_decimal_cost_is_deterministic,malformed_or_negative_prices_are_rejected}` 通过。 |
| 无公网测试 | OAuth 测试仅使用 `127.0.0.1` fixture URL 和内存响应，不创建 HTTP client，不启动浏览器。 |

| Command | Exit | Result |
|---|---:|---|
| `cd src-tauri && cargo fmt --check` | 0 | Rust 格式检查通过。 |
| `cd src-tauri && cargo test ai_routing_gateway --lib` | 0 | 26 passed, 0 failed, 0 ignored。 |
| `cd src-tauri && cargo check` | 0 | 编译检查通过，无 warning。 |
| `cd src-tauri && cargo test --lib` | 0 | 391 passed, 0 failed, 2 ignored；ignored 为既有本机环境 smoke test。 |

## Out of Scope

不实现网关 Key、HTTP 端点、请求路由、Tauri commands、前端或生命周期接线。
