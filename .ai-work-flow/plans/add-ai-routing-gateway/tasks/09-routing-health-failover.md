# 09 - 实现路由选择、健康状态与故障切换

- task_id: `09-routing-health-failover`
- order: `09`
- blocked_by: `04-official-codex-oauth, 05-quota-threshold-pricing, 06-gateway-api-key-security, 07-protocol-conversion-sse, 08-request-logging-aggregation`
- source_plan: `../plan.md`
- source_plan_digest: `385b139e1c25f8e8112982ed63ac3c3f0282be095c8322006f82f45d9070cf6d`
- write_scope: `src-tauri/src/ai_routing_gateway/{router.rs,health.rs,upstream.rs,tests/router.rs}`

## Outcome

每个请求都能按 Key 权限、模型映射、健康和额度规则稳定选择账号，并在限定条件下最多尝试三个账号及维护可恢复的健康状态。

## Implementation Checklist

- [ ] 实现候选快照及分组/模型授权、启用、凭据、映射、健康和适用额度窗口过滤。
- [ ] 实现账号排序、最低剩余比例、额度新鲜度和最近使用时间的确定性排序及更新。
- [ ] 实现每请求最多三个不同账号、首字节前切换和首字节后固定账号。
- [ ] 实现 OAuth `401/403` 一次互斥刷新、第三方授权失效及其他业务 `4xx` 不降健康度规则。
- [ ] 实现额度耗尽暂停、`429` 冷却、网络/`5xx` 熔断、指数上限和单探测恢复。
- [ ] 将每次尝试和最终逻辑结果接入脱敏日志，并生成统一候选耗尽错误。

## Acceptance Criteria

- [ ] 候选过滤完整执行所有权限、账号、映射、健康和额度条件，标签不影响结果。
- [ ] 排序测试覆盖用户排序、最低剩余比例、过期快照降级和最近使用时间的稳定决胜。
- [ ] 最多调用三个不同账号；只有尚未向客户端输出任何字节时允许切换。
- [ ] OAuth 与第三方 `401/403`、额度耗尽、带或不带 `Retry-After` 的 `429`、网络错误和 `5xx` 均触发规定行为。
- [ ] 连续三次网络或 `5xx` 后熔断，冷却最长 15 分钟；到期仅允许一个探测并按结果恢复或重新熔断。
- [ ] 客户端取消不计为上游健康失败；普通请求或模型 `4xx` 不切换账号。
- [ ] 候选耗尽返回 `no_available_upstream`，不泄露账号清单或过滤细节。

## Verification Steps

- [ ] 使用本机 mock upstream 执行候选过滤、稳定排序和最多三次尝试测试。
- [ ] 执行完整错误分类、冷却、熔断、单探测和恢复时序测试。
- [ ] 执行首字节前后切换、客户端取消及尝试日志一致性测试。

## Out of Scope

不监听 HTTP 端口、不扩展 Protocol Router，也不提供超过三个账号的重试策略。
