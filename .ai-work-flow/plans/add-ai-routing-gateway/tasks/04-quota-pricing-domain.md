# 04 - 实现额度与价格领域

- task_id: `ai-routing-quota-pricing`
- order: `04`
- blocked_by: `ai-routing-account-catalog`
- source_plan: `../plan.md`
- source_plan_digest: `4fd0f21d03c9fd3060e030ce14e81636333e3060e47dee01c6720c5983f985ce`
- write_scope: `src-tauri/src/ai_routing_gateway/usage/**；src-tauri/src/ai_routing_gateway/pricing.rs；src-tauri/src/ai_routing_gateway/storage/{quota.rs,pricing.rs,settings.rs}；src-tauri/src/ai_routing_gateway/types/{quota.rs,pricing.rs}；对应 Rust 测试与额度 fixtures`

## Outcome

OAuth 账号的动态额度、阈值、刷新状态和请求时价格快照可按确定规则计算、持久化与查询。

## Implementation Checklist

- [ ] 实现额度窗口解析、规范化、作用域匹配和持久化。
- [ ] 实现登录、手动、请求完成及五分钟周期刷新入口。
- [ ] 实现同账号刷新合并、上限指数退避和最后成功快照。
- [ ] 实现全局阈值与账号继承或覆盖。
- [ ] 实现基础、附加和未知窗口的门禁规则。
- [ ] 初始化官方模型价格并实现第三方覆盖及不可变请求价格快照。
- [ ] 实现首页额度聚合计算接口。

## Acceptance Criteria

- [ ] 阈值范围为 `0-100`，默认 `10%`；`0` 只在完全耗尽时阻止账号。
- [ ] 任一适用基础窗口低于阈值时仅阻止对应请求；附加窗口只限制对应能力。
- [ ] 有明确范围的未知窗口参与同类门禁，无范围未知窗口只展示。
- [ ] 额度快照过期只降低候选排序，不直接停用账号。
- [ ] 阈值提交后立即影响后续请求，不终止在途请求。
- [ ] 刷新失败保留最后成功快照并标记过期；同账号并发刷新合并。
- [ ] 首页 5 小时和 7 日比例只对拥有对应窗口且当前可路由的 OAuth 账号做算术平均。
- [ ] 第三方账号不进入额度平均；附加窗口按规范化名称分别聚合。
- [ ] 价格优先级固定为匹配的第三方覆盖优先于官方默认。
- [ ] 缺价格或缺必要用量时费用状态为不可计算，不写为 `0`。

## Verification Steps

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::usage`。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::pricing`。
- [ ] 覆盖 5 小时、7 日、仅 7 日、Code Review、Spark 和未知窗口 fixtures。
- [ ] 覆盖阈值 `0`、`10`、`100`、等于阈值、低于阈值、继承、覆盖和自动恢复。
- [ ] 用假时钟验证五分钟调度、指数退避、过期降级及并发合并。
- [ ] 验证官方价格、第三方覆盖、请求时快照、改价不追溯和不可计算状态。
- [ ] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`。

## Out of Scope

不实现 OAuth token 获取、网关 Key、HTTP 路由、日志聚合或前端图表。
