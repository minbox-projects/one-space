# 04 - 实现官方 Codex OAuth 三种授权路径

- task_id: `04-official-codex-oauth`
- order: `04`
- blocked_by: `02-keychain-credential-security, 03-account-pool-model-mapping`
- source_plan: `../plan.md`
- source_plan_digest: `037804aa9bfa9cdfc9001966bb673f99116f870c328e29c2f1e5ad7aa4c79d19`
- write_scope: `src-tauri/src/ai_routing_gateway/{oauth.rs,oauth_sessions.rs,tests/oauth.rs}`

## Outcome

用户可通过系统浏览器 loopback、手动粘贴完整回调 URL 或 Device Code 完成受控的官方 Codex OAuth 登录，且授权材料不落盘、账号按稳定 ID 去重。

## Implementation Checklist

- [ ] 建立仅允许受控官方授权端点、固定 scope、PKCE 和官方刷新语义的内存授权会话。
- [ ] 实现随机 loopback 回调端口、系统浏览器打开和自动回调处理。
- [ ] 实现与自动回调共用 state/code/error 校验逻辑的手动完整回调 URL 路径。
- [ ] 实现 Device Code 状态机，包括服务端间隔、pending、slow_down、expired、取消和成功。
- [ ] 成功后按官方稳定账号 ID 原子 upsert 账号及加密凭据，再授权时替换而不重复建号。
- [ ] 实现同账号 Token 刷新合并、一次刷新重试和授权失效处理，并在应用退出时清理所有临时会话。

## Acceptance Criteria

- [ ] 授权 URL 使用受控官方端点、PKCE challenge 和固定 scope，不允许自定义 issuer、client ID 或 scope。
- [ ] 自动与手动回调均校验 state；错误 state、上游 error、过期和取消会终止并清理会话。
- [ ] loopback 监听失败后，会话有效期内仍可使用手动完整回调 URL。
- [ ] Device Code 严格遵循服务端间隔，`slow_down` 增加后续间隔，所有终止状态均停止轮询。
- [ ] code、device code、PKCE verifier 和 state 不进入 SQLite、日志或持久化配置。
- [ ] 同一稳定账号 ID 再授权只更新凭据和元数据；并发刷新只执行一次上游刷新。
- [ ] 全部测试使用本机 mock server，不访问公网，也不采用 Cookie、网页抓取或浏览器会话模拟。

## Verification Steps

- [ ] 执行 OAuth loopback 与手动回调测试，覆盖成功、state 失败、上游错误和会话清理。
- [ ] 执行 Device Code 时序测试以及 Token 刷新合并、重试和账号去重测试。
- [ ] 执行敏感材料持久化扫描测试。

## Out of Scope

不实现自定义 OAuth 提供方、Cookie 导入、网页抓取或绕过官方政策限制的替代登录流程。
