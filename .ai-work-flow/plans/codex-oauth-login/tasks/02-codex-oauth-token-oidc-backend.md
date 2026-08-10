# 实现授权码交换与 OIDC 身份验证后端

## 预期结果

依赖任务 01；在严格回调校验后交换 token，验证可信 id_token 声明并显式记录 JWKS 验证降级，按 chatgpt_account_id、workspace_id 与 sub 回退规则生成稳定身份；任何交换、解析、验证或主体映射失败均不得产生成功账号。该任务已整合至 integration，后续不得重新实施。

## 实施清单

- [x] 已完成并整合至 `integration`；后续实施不得重做或改写本任务的生产实现。
- [x] 已在 OAuth 服务边界中使用公开 client_id、authorization code、PKCE verifier 和原始 redirect URI 交换 token，且不发送 client_secret。
- [x] 已实现 token 响应安全解析、可信 JWKS 签名与 `exp`、`iss`、`aud`、nonce 校验，以及缺少可信 JWKS 时的可见降级。
- [x] 已按 `chatgpt_account_id`、可选 `workspace_id` 与 `sub` 回退规则映射稳定主体，并拒绝任何不完整或不可信成功结果。
- [x] 已将所需 Rust 依赖限制在 OIDC 验证边界，并同步维护 `Cargo.toml` 与 `Cargo.lock`。

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
