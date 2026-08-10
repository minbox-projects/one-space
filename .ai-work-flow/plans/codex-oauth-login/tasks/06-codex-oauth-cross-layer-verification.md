# 06 - 完成 Codex OAuth 跨层测试与回归验证

- task_id: `codex-oauth-cross-layer-verification`
- order: `06`
- blocked_by: `codex-oauth-provider-protocol, codex-oauth-token-oidc-backend, codex-oauth-credential-lifecycle, codex-oauth-tauri-typed-ipc, codex-oauth-account-pool-ui`
- source_plan: `../plan.md`
- source_plan_digest: `fc6ad1badf9a727823ad816a313d104a55883c62b8c8767fbe019aae3ddc029e`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src-tauri/src/ai_routing_gateway/oauth.rs`
  - `src-tauri/src/ai_routing_gateway/accounts.rs`
  - `src-tauri/src/ai_routing_gateway/runtime.rs`
  - `src-tauri/src/ai_routing_gateway/commands/mod.rs`
  - `src-tauri/src/ai_routing_gateway/tests.rs`
  - `src-tauri/src/app_runtime/run_app.rs`
  - `src/lib/aiRoutingGateway.test.ts`
  - `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`
  - `src/i18n.test.ts`

## AI Work Flow Task Metadata

```json
{
  "plan_id": "codex-oauth-login",
  "plan_digest": "fc6ad1badf9a727823ad816a313d104a55883c62b8c8767fbe019aae3ddc029e",
  "preview_revision": 1,
  "task_id": "codex-oauth-cross-layer-verification",
  "order": 6,
  "title": "完成 Codex OAuth 跨层测试与回归验证",
  "summary": "补齐 Rust 协议、回调、token、OIDC、身份去重、加密持久化、rotation、路由刷新重试、退出登录测试，验证 Tauri command 注册、typed IPC DTO 与无敏感字段事件，并覆盖 React 登录状态机、手动回调、成功刷新和只读连接信息；执行 Rust、前端定向及全量测试、lint 和构建，确认 API Key、Gmail OAuth 与网关 Bootstrap 行为无回归。",
  "blocked_by": [
    "codex-oauth-provider-protocol",
    "codex-oauth-token-oidc-backend",
    "codex-oauth-credential-lifecycle",
    "codex-oauth-tauri-typed-ipc",
    "codex-oauth-account-pool-ui"
  ],
  "write_scope_mode": "exhaustive",
  "write_scope": [
    "src-tauri/src/ai_routing_gateway/oauth.rs",
    "src-tauri/src/ai_routing_gateway/accounts.rs",
    "src-tauri/src/ai_routing_gateway/runtime.rs",
    "src-tauri/src/ai_routing_gateway/commands/mod.rs",
    "src-tauri/src/ai_routing_gateway/tests.rs",
    "src-tauri/src/app_runtime/run_app.rs",
    "src/lib/aiRoutingGateway.test.ts",
    "src/components/AiRoutingGateway/AiRoutingGateway.test.tsx",
    "src/i18n.test.ts"
  ],
  "acceptance": [
    "Rust 测试可判定地覆盖 callback 校验先于 token 交换、可信 OIDC 验证、降级可见性、稳定身份、加密落库、rotation、刷新重试和退出登录。",
    "Tauri command 注册和 typed facade 的命令名、参数名、序列化字段及事件状态完全一致，敏感字段负向断言通过。",
    "React 测试覆盖完整登录状态机、手动 callback、成功刷新、重新授权和 OAuth 连接字段只读，现有 API Key 交互测试保持通过。",
    "`cargo test`、前端定向测试、`npm test`、`npm run lint` 和 `npm run build` 全部成功。",
    "Gmail OAuth、API Key 与 gateway bootstrap 的既有测试无回归，代码与测试日志不含 token、authorization code、PKCE verifier 或完整 callback URL。"
  ]
}
```

## 预期结果

补齐 Rust 协议、回调、token、OIDC、身份去重、加密持久化、rotation、路由刷新重试、退出登录测试，验证 Tauri command 注册、typed IPC DTO 与无敏感字段事件，并覆盖 React 登录状态机、手动回调、成功刷新和只读连接信息；执行 Rust、前端定向及全量测试、lint 和构建，确认 API Key、Gmail OAuth 与网关 Bootstrap 行为无回归。

## 实施清单

- [ ] 补齐 Rust OAuth 协议测试：provider 参数、PKCE S256、随机 loopback/state/nonce、自动与手动 callback 共用校验、TTL、取消、错误和重放。
- [ ] 补齐 token/OIDC 测试：请求参数、交换失败、畸形响应、可信 JWKS 签名、exp/iss/aud/nonce、无可信 JWKS 降级以及可靠主体缺失拒绝。
- [ ] 补齐账号生命周期测试：`chatgpt_account_id` 去重、workspace 隔离、`sub` 回退、AES-GCM 明文无泄漏、事务失败回滚、refresh rotation 和退出登录本地优先语义。
- [ ] 补齐 runtime 测试：到期前刷新、授权失败最多一次刷新与一次原请求重试、临时错误有限退避、永久失败重新授权标记和候选剔除。
- [ ] 补齐 Tauri/IPC 测试：command 注册清单、Rust/TypeScript DTO 参数一致、listener 失败手动回退、状态事件终态以及所有公开 payload 无敏感字段。
- [ ] 补齐 React 测试：OAuth/API Key 并列入口、等待、取消、超时、错误、手动完整 callback、成功 bootstrap 刷新、重新授权、退出登录和 OAuth 连接字段只读。
- [ ] 执行并修复定向、全量、lint 和构建回归，确认 API Key 创建/编辑/路由、Gmail OAuth 及网关 Bootstrap 行为未改变。

## 验收标准

- [ ] Rust 测试可判定地覆盖 callback 校验先于 token 交换、可信 OIDC 验证、降级可见性、稳定身份、加密落库、rotation、刷新重试和退出登录。
- [ ] Tauri command 注册和 typed facade 的命令名、参数名、序列化字段及事件状态完全一致，敏感字段负向断言通过。
- [ ] React 测试覆盖完整登录状态机、手动 callback、成功刷新、重新授权和 OAuth 连接字段只读，现有 API Key 交互测试保持通过。
- [ ] `cargo test`、前端定向测试、`npm test`、`npm run lint` 和 `npm run build` 全部成功。
- [ ] Gmail OAuth、API Key 与 gateway bootstrap 的既有测试无回归，代码与测试日志不含 token、authorization code、PKCE verifier 或完整 callback URL。

## 验证步骤

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml`，预期全部 Rust 单元与集成测试通过。
- [ ] 运行 `npm test -- --run src/lib/aiRoutingGateway.test.ts src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`，预期 typed IPC 与 React OAuth 定向测试通过。
- [ ] 运行 `npm test`，预期包括 Gmail OAuth、API Key 和网关 Bootstrap 在内的前端全量测试通过。
- [ ] 运行 `npm run lint`，预期 ESLint 无错误。
- [ ] 运行 `npm run build`，预期 TypeScript 与生产构建成功。
- [ ] 审查测试输出和失败快照，预期不出现 access/refresh/id token、authorization code、PKCE verifier 或完整 callback URL。

## 范围外事项

- 不新增已确认六项任务以外的产品能力、迁移或界面重构。
- 不以降低 OIDC 校验、明文保存凭据或增加无限重试的方式修复测试。
