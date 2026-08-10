# 02 - 实现真实授权码交换与 OIDC 身份验证后端

- task_id: `codex-oauth-token-oidc-backend`
- order: `02`
- blocked_by: `codex-oauth-provider-protocol`
- source_plan: `../plan.md`
- source_plan_digest: `815daa835342a70583c59738ccaef385c69f9e7f6b54c2fec04cf2d093e79be7`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src-tauri/Cargo.toml`
  - `src-tauri/Cargo.lock`
  - `src-tauri/src/ai_routing_gateway/oauth.rs`
  - `src-tauri/src/ai_routing_gateway/accounts.rs`
  - `src-tauri/src/ai_routing_gateway/commands/mod.rs`

## AI Work Flow Task Metadata

```json
{
  "plan_id": "codex-oauth-login",
  "plan_digest": "815daa835342a70583c59738ccaef385c69f9e7f6b54c2fec04cf2d093e79be7",
  "preview_revision": 2,
  "task_id": "codex-oauth-token-oidc-backend",
  "order": 2,
  "title": "实现真实授权码交换与 OIDC 身份验证后端",
  "summary": "在严格回调校验成功后执行真实 authorization code 与 PKCE verifier 的 token 交换，验证可信 id_token 的 exp、iss、aud 和 nonce，并在缺少可信 JWKS 时记录可见验证降级；按 chatgpt_account_id、可选 workspace_id 和 sub 回退规则生成稳定身份，无可靠主体或任一交换、解析、验证失败时拒绝产生成功账号。",
  "blocked_by": [
    "codex-oauth-provider-protocol"
  ],
  "write_scope_mode": "exhaustive",
  "write_scope": [
    "src-tauri/Cargo.toml",
    "src-tauri/Cargo.lock",
    "src-tauri/src/ai_routing_gateway/oauth.rs",
    "src-tauri/src/ai_routing_gateway/accounts.rs",
    "src-tauri/src/ai_routing_gateway/commands/mod.rs"
  ],
  "acceptance": [
    "token 请求包含 code、PKCE verifier、公开 client_id、redirect URI 和授权码 grant type，且严格发生在 callback 校验成功之后。",
    "可信 JWKS 路径拒绝签名、`exp`、`iss`、`aud`、nonce 或 `kid` 不匹配的 id_token；验证降级状态仅在确实没有可信 JWKS 时可见地产生。",
    "`chatgpt_account_id + workspace_id`、`chatgpt_account_id` 和 `sub` 回退规则生成确定且隔离的稳定外部身份，email/name 不参与身份主键。",
    "缺少可靠主体或交换、解析、验证任一步失败时，不调用账号成功 upsert，也不返回成功账号。",
    "token、refresh token、authorization code、PKCE verifier、id_token 和完整 callback URL 不进入日志、公开 metadata、command DTO 或事件。"
  ]
}
```

## 预期结果

在严格回调校验成功后执行真实 authorization code 与 PKCE verifier 的 token 交换，验证可信 id_token 的 exp、iss、aud 和 nonce，并在缺少可信 JWKS 时记录可见验证降级；按 chatgpt_account_id、可选 workspace_id 和 sub 回退规则生成稳定身份，无可靠主体或任一交换、解析、验证失败时拒绝产生成功账号。

## 执行状态

已完成并整合至 `integration`，后续不得重新实施；保留以下技术验收作为既有实现的验证依据。

## 实施清单

- [ ] 在后端 OAuth 服务边界中使用 provider token endpoint、公开 client_id、authorization code、PKCE verifier 和原始 redirect URI提交标准 token 请求，不发送 client_secret。
- [ ] 解析成功 token 响应为内部凭据包，要求路由所需 access token，并对错误 HTTP 状态、畸形 JSON、缺失字段和异常过期信息返回安全分类错误。
- [ ] 解析 `id_token` header 与 claims；存在可信 JWKS 时按 `kid` 选钥并验证签名、`exp`、`iss`、`aud` 和会话 nonce。
- [ ] 无法取得可信 JWKS 时显式产生可观察的验证降级状态，不把仅解析 claims 表述为完整 OIDC 验证；网络或解析失败不得静默绕过已配置的可信验证。
- [ ] 优先以 `chatgpt_account_id` 生成稳定主体；存在 `workspace_id` 时纳入隔离键；仅当前者缺失时回退 OIDC `sub`，email 和 name 只作展示。
- [ ] 只有 callback、交换、claims 验证和主体映射全部成功后，才把内部 token bundle 与稳定身份交给账号 upsert；任何失败均不得创建成功账号。
- [ ] 如签名/JWK 解析需要新增 Rust 依赖，同步维护 `Cargo.toml` 与 `Cargo.lock`，限制依赖用途在可信 OIDC 验证边界。

## 验收标准

- [ ] token 请求包含 code、PKCE verifier、公开 client_id、redirect URI 和授权码 grant type，且严格发生在 callback 校验成功之后。
- [ ] 可信 JWKS 路径拒绝签名、`exp`、`iss`、`aud`、nonce 或 `kid` 不匹配的 id_token；验证降级状态仅在确实没有可信 JWKS 时可见地产生。
- [ ] `chatgpt_account_id + workspace_id`、`chatgpt_account_id` 和 `sub` 回退规则生成确定且隔离的稳定外部身份，email/name 不参与身份主键。
- [ ] 缺少可靠主体或交换、解析、验证任一步失败时，不调用账号成功 upsert，也不返回成功账号。
- [ ] token、refresh token、authorization code、PKCE verifier、id_token 和完整 callback URL 不进入日志、公开 metadata、command DTO 或事件。

## 验证步骤

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::oauth::tests`，预期 token 请求、JWKS、claims、nonce、降级和身份映射用例全部通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::accounts`，预期稳定身份输入与拒绝不完整凭据的测试通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml`，预期新增依赖锁定一致且 Rust 全量测试通过。

## 范围外事项

- 不实现 Device Code、client_secret 或 API-key token exchange。
- 不负责路由前刷新、refresh token rotation、Tauri 注册或前端登录状态机。
