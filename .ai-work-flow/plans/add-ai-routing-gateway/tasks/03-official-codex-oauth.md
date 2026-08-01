# 03 - 实现官方 Codex OAuth 流程

- task_id: `ai-routing-oauth`
- order: `03`
- blocked_by: `ai-routing-account-catalog`
- source_plan: `../plan.md`
- source_plan_digest: `4fd0f21d03c9fd3060e030ce14e81636333e3060e47dee01c6720c5983f985ce`
- write_scope: `src-tauri/src/ai_routing_gateway/oauth/**；src-tauri/src/ai_routing_gateway/storage/oauth.rs；src-tauri/src/ai_routing_gateway/types/oauth.rs；对应 Rust 测试与本机 mock fixtures`

## Outcome

用户可通过 loopback、手动完整回调 URL 或 Device Code 完成受控官方 Codex OAuth，且授权材料、刷新和账号去重满足安全规则。

## Implementation Checklist

- [ ] 固化已审定的官方 Codex 授权端点、token 端点、PKCE 和 scope。
- [ ] 实现内存授权会话、随机 loopback 回调端口和可注入的系统浏览器启动器。
- [ ] 让自动回调和手动完整回调 URL 共用 state/code/error 校验。
- [ ] 实现 Device Code 状态机和服务端间隔驱动的轮询。
- [ ] 实现稳定账号 ID upsert、凭据原子替换和会话清理。
- [ ] 实现同账号 token 刷新互斥及一次授权失败重试语义。

## Acceptance Criteria

- [ ] 不支持自定义 issuer、client ID 或 scope，不导入 Cookie，不抓取网页。
- [ ] loopback 只绑定本机随机可用端口；测试使用浏览器启动器替身，不打开真实浏览器。
- [ ] state 不匹配、过期、取消或 OAuth error 均拒绝兑换并清理临时材料。
- [ ] 自动回调监听失败时，会话在有效期内仍接受手动完整回调 URL。
- [ ] Device Code 严格处理 `authorization_pending`、`slow_down`、过期、取消和成功；`slow_down` 增加后续间隔。
- [ ] code、device code、PKCE verifier 和 state 不写入数据库、日志或事件。
- [ ] 同一稳定账号 ID 再授权只原子更新账号元数据与凭据，不创建重复账号。
- [ ] 同账号并发刷新只发起一次上游刷新；OAuth `401/403` 最多刷新并重试一次。
- [ ] 官方政策或技术接入条件无法确认时，该能力保持禁用并标记发布阻塞，不使用替代抓取方案。

## Verification Steps

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::oauth`。
- [ ] 使用本机 mock server 验证授权 URL、PKCE、固定 scope、state、手动回调和 token 兑换。
- [ ] 用暂停时间的测试覆盖 Device Code 间隔、`pending`、`slow_down`、过期和取消。
- [ ] 并发测试刷新合并、一次重试、授权失效及稳定账号 ID 去重。
- [ ] 扫描测试数据库和捕获日志，确认临时授权材料未持久化。
- [ ] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`。

## Out of Scope

不实现网页抓取、自定义 OAuth、额度窗口解析、HTTP 网关或 OAuth UI。
