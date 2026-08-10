# 01 - 建立 Codex OAuth provider 配置与授权码协议模型

- task_id: `codex-oauth-provider-protocol`
- order: `01`
- blocked_by: `none`
- source_plan: `../plan.md`
- source_plan_digest: `fc6ad1badf9a727823ad816a313d104a55883c62b8c8767fbe019aae3ddc029e`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src-tauri/src/ai_routing_gateway/oauth.rs`

## AI Work Flow Task Metadata

```json
{
  "plan_id": "codex-oauth-login",
  "plan_digest": "fc6ad1badf9a727823ad816a313d104a55883c62b8c8767fbe019aae3ddc029e",
  "preview_revision": 1,
  "task_id": "codex-oauth-provider-protocol",
  "order": 1,
  "title": "建立 Codex OAuth provider 配置与授权码协议模型",
  "summary": "集中定义可替换的 Codex-Manager 兼容授权端点、token 端点、公开 client_id、scope、兼容参数、issuer、audience、JWKS 与可选 revoke endpoint，并扩展 OAuth 会话的随机 loopback、PKCE S256、state、nonce、TTL、一次性消费及自动与手动回调共用的严格校验模型；明确兼容风险且不引入 client_secret、Device Code 或 API-key token exchange。",
  "blocked_by": [],
  "write_scope_mode": "exhaustive",
  "write_scope": [
    "src-tauri/src/ai_routing_gateway/oauth.rs"
  ],
  "acceptance": [
    "Codex OAuth 所有兼容端点和参数只在一个 provider 配置边界中定义，修改兼容契约不需要改动 React 或账号持久化逻辑。",
    "授权 URL 固定使用 `response_type=code`、PKCE `S256`、随机 state、随机 nonce 和随机 loopback redirect URI，且不含 client_secret。",
    "自动与手动 callback 对 origin、端口、path、state、TTL 和授权错误执行完全相同的严格校验，成功或失败后均不能重放会话。",
    "Codex 登录生产路径不存在 Device Code 和 API-key token exchange 调用，Gmail OAuth 与 API Key 路径未被修改。",
    "Debug、Display 和错误信息不包含 authorization code、PKCE verifier 或完整 callback URL。"
  ]
}
```

## 预期结果

集中定义可替换的 Codex-Manager 兼容授权端点、token 端点、公开 client_id、scope、兼容参数、issuer、audience、JWKS 与可选 revoke endpoint，并扩展 OAuth 会话的随机 loopback、PKCE S256、state、nonce、TTL、一次性消费及自动与手动回调共用的严格校验模型；明确兼容风险且不引入 client_secret、Device Code 或 API-key token exchange。

## 实施清单

- [ ] 在 `oauth.rs` 中建立单一 Codex provider 配置，完整承载 authorization、token、issuer、audience、JWKS、公开 client_id、scope、兼容参数和可选 revoke endpoint，并使兼容配置可集中替换。
- [ ] 移除生产发布阻断默认值，显式记录 Codex-Manager 契约并非官方稳定第三方契约，且配置模型不接受或生成 client_secret。
- [ ] 将本功能路径收敛到 Authorization Code + PKCE S256，删除或隔离 Device Code 与 API-key token exchange 的生产入口。
- [ ] 为每个会话生成随机非零 loopback 端口绑定所需材料、state、nonce 和 PKCE verifier，保留十分钟短 TTL、取消清理及一次性消费语义。
- [ ] 让自动 listener callback 与手动粘贴的完整 callback URL 汇合到相同校验函数，严格检查 session、TTL、HTTP loopback origin、端口、path、state、授权错误和非空 code。
- [ ] 保证完成结果仅供后端后续交换使用，并携带 code、PKCE verifier、nonce、redirect URI 和 provider 上下文，不扩大任何公开 DTO。

## 验收标准

- [ ] Codex OAuth 所有兼容端点和参数只在一个 provider 配置边界中定义，修改兼容契约不需要改动 React 或账号持久化逻辑。
- [ ] 授权 URL 固定使用 `response_type=code`、PKCE `S256`、随机 state、随机 nonce 和随机 loopback redirect URI，且不含 client_secret。
- [ ] 自动与手动 callback 对 origin、端口、path、state、TTL 和授权错误执行完全相同的严格校验，成功或失败后均不能重放会话。
- [ ] Codex 登录生产路径不存在 Device Code 和 API-key token exchange 调用，Gmail OAuth 与 API Key 路径未被修改。
- [ ] Debug、Display 和错误信息不包含 authorization code、PKCE verifier 或完整 callback URL。

## 验证步骤

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::oauth::tests`，预期 PKCE、随机值、严格 callback、超时、取消和重放用例全部通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml`，预期 Rust 测试全部通过且无新增编译警告。
- [ ] 检查定向测试生成的授权 URL 与序列化错误，预期不存在 `client_secret`、Device Code、API-key token exchange 或敏感会话材料。

## 范围外事项

- 不实现 token 网络交换、OIDC 签名验证、账号落库、Tauri command 注册或 React 登录界面。
- 不将 Codex-Manager 兼容契约表述为 OpenAI 官方稳定第三方契约。
