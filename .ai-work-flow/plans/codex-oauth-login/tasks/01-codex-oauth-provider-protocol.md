# 建立 Codex OAuth Provider 配置与授权码协议模型

## 预期结果

无前置依赖；集中定义可替换的 Codex-Manager 兼容 Provider 配置，并完善随机 loopback、PKCE S256、state、nonce、TTL、一次性会话及自动与手动回调共用的严格校验模型；禁止引入 client_secret、Device Code 或 API-key token exchange。该任务已整合至 integration，后续不得重新实施。

## 实施清单

- [x] 已完成并整合至 `integration`；后续实施不得重做或改写本任务的生产实现。
- [x] 已在 `oauth.rs` 中建立单一 Codex provider 配置，集中承载 authorization、token、issuer、audience、JWKS、公开 client_id、scope、兼容参数和可选 revoke endpoint。
- [x] 已将 Codex 登录收敛到 Authorization Code + PKCE S256，并隔离 Device Code、API-key token exchange 和 client_secret。
- [x] 已实现随机 loopback、state、nonce、PKCE verifier、短 TTL、取消清理与一次性消费语义。
- [x] 已让自动 listener callback 与手动完整 callback URL 汇合到相同严格校验路径，且不扩大公开 DTO 或泄露敏感材料。

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
