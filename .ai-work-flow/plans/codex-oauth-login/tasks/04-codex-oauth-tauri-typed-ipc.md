# 04 - 注册 Tauri OAuth commands 并完善 typed IPC 契约

- task_id: `codex-oauth-tauri-typed-ipc`
- order: `04`
- blocked_by: `codex-oauth-provider-protocol, codex-oauth-token-oidc-backend, codex-oauth-credential-lifecycle`
- source_plan: `../plan.md`
- source_plan_digest: `fc6ad1badf9a727823ad816a313d104a55883c62b8c8767fbe019aae3ddc029e`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src-tauri/src/ai_routing_gateway/oauth.rs`
  - `src-tauri/src/ai_routing_gateway/commands/mod.rs`
  - `src-tauri/src/app_runtime/run_app.rs`
  - `src/lib/aiRoutingGateway.ts`

## AI Work Flow Task Metadata

```json
{
  "plan_id": "codex-oauth-login",
  "plan_digest": "fc6ad1badf9a727823ad816a313d104a55883c62b8c8767fbe019aae3ddc029e",
  "preview_revision": 1,
  "task_id": "codex-oauth-tauri-typed-ipc",
  "order": 4,
  "title": "注册 Tauri OAuth commands 并完善 typed IPC 契约",
  "summary": "将开始登录、自动或手动完成回调、取消和退出登录 commands 注册到 Tauri invoke handler，编排临时 loopback listener 与系统浏览器并在 listener 失败时保留手动回调会话；同步 Rust DTO、状态事件和 TypeScript facade，确保参数与序列化一致且 token、authorization code、PKCE verifier 和完整 callback URL 不通过 IPC、事件或日志泄露。",
  "blocked_by": [
    "codex-oauth-provider-protocol",
    "codex-oauth-token-oidc-backend",
    "codex-oauth-credential-lifecycle"
  ],
  "write_scope_mode": "exhaustive",
  "write_scope": [
    "src-tauri/src/ai_routing_gateway/oauth.rs",
    "src-tauri/src/ai_routing_gateway/commands/mod.rs",
    "src-tauri/src/app_runtime/run_app.rs",
    "src/lib/aiRoutingGateway.ts"
  ],
  "acceptance": [
    "所有 Codex OAuth commands 均可通过 Tauri invoke handler 调用，Rust DTO 字段名与 TypeScript facade 参数完全一致。",
    "自动 loopback 成功会完成登录；listener 失败后同一 session 仍可手动完成；取消、超时和终态错误会释放 listener 并清理会话。",
    "begin command 能打开系统浏览器，并在浏览器打开失败时返回可恢复状态而不泄漏或错误消费 session。",
    "IPC 结果和 OAuth 事件不包含 access/refresh/id token、authorization code、PKCE verifier 或完整 callback URL。",
    "退出登录 command 调用既有本地清除/禁用生命周期并返回非敏感结果。"
  ]
}
```

## 预期结果

将开始登录、自动或手动完成回调、取消和退出登录 commands 注册到 Tauri invoke handler，编排临时 loopback listener 与系统浏览器并在 listener 失败时保留手动回调会话；同步 Rust DTO、状态事件和 TypeScript facade，确保参数与序列化一致且 token、authorization code、PKCE verifier 和完整 callback URL 不通过 IPC、事件或日志泄露。

## 实施清单

- [ ] 将 OAuth store 以生产 Codex provider 配置初始化，并在 `generate_handler!` 注册开始登录、完成手动回调、取消和退出登录 commands。
- [ ] 开始登录时先绑定随机本地端口并创建 session，再启动临时 loopback listener 和系统浏览器；listener 负责消费自动 callback 并汇合到同一后端完成服务。
- [ ] listener 启动或运行失败时发出安全状态并保留未过期 session，允许用户通过手动 command 提交完整 callback；取消、超时和终态错误清理 listener 与 session。
- [ ] 设计 Rust 输入/输出 DTO 与 OAuth 状态事件，仅传递 session ID、非敏感状态、授权 URL、过期信息和成功账号标识；完整 callback URL 只允许作为手动 command 输入并不得回显。
- [ ] 在 `aiRoutingGateway.ts` 增加与 Rust camelCase 序列化一致的 begin、manual complete、cancel、logout facade、结果类型和等待/成功/取消/超时/失败状态事件。
- [ ] 对 command 错误做安全分类，确保日志、Tauri invoke 返回和事件均不包含 token、authorization code、PKCE verifier 或完整 callback URL。

## 验收标准

- [ ] 所有 Codex OAuth commands 均可通过 Tauri invoke handler 调用，Rust DTO 字段名与 TypeScript facade 参数完全一致。
- [ ] 自动 loopback 成功会完成登录；listener 失败后同一 session 仍可手动完成；取消、超时和终态错误会释放 listener 并清理会话。
- [ ] begin command 能打开系统浏览器，并在浏览器打开失败时返回可恢复状态而不泄漏或错误消费 session。
- [ ] IPC 结果和 OAuth 事件不包含 access/refresh/id token、authorization code、PKCE verifier 或完整 callback URL。
- [ ] 退出登录 command 调用既有本地清除/禁用生命周期并返回非敏感结果。

## 验证步骤

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::commands`，预期 command DTO、listener 编排、事件脱敏与退出登录测试通过。
- [ ] 运行 `npm test -- --run src/lib/aiRoutingGateway.test.ts`，预期 facade 命令名、参数序列化、事件类型和敏感字段约束测试通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml`，预期 `run_app.rs` command 注册与 Rust 全量测试通过。

## 范围外事项

- 不实现 React 账号池对话框和本地化视觉文案。
- 不通过 IPC 暴露任何可用于重放授权或访问上游的秘密材料。
