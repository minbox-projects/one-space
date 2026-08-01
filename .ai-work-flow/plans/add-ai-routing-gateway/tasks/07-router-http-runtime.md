# 07 - 实现路由与 HTTP 运行时

- task_id: `ai-routing-router-http`
- order: `07`
- blocked_by: `ai-routing-oauth, ai-routing-quota-pricing, ai-routing-gateway-keys, ai-routing-protocol`
- source_plan: `../plan.md`
- source_plan_digest: `4fd0f21d03c9fd3060e030ce14e81636333e3060e47dee01c6720c5983f985ce`
- write_scope: `src-tauri/src/ai_routing_gateway/router/**；src-tauri/src/ai_routing_gateway/runtime/http.rs；src-tauri/src/ai_routing_gateway/types/{routing.rs,http.rs}；对应 Rust loopback 集成测试`

## Outcome

独立本地服务可在 IPv4 loopback 上鉴权并处理四个 HTTP 端点，按确定候选、失败切换和健康状态规则调用上游。

## Implementation Checklist

- [ ] 实现 Bearer 鉴权中间件、输入大小限制和脱敏错误映射。
- [ ] 实现 `/health`、`/v1/models`、`/v1/responses` 和 `/v1/chat/completions`。
- [ ] 实现候选过滤、不可变候选快照和稳定排序。
- [ ] 实现最多三账号尝试及首字节切换门禁。
- [ ] 实现 OAuth/第三方授权失效、额度暂停、429 冷却和网络/5xx 熔断。
- [ ] 实现单探测恢复和最近使用状态更新。
- [ ] 为后续持久日志提供结构化请求与尝试事件接口。

## Acceptance Criteria

- [ ] listener 只能绑定 `127.0.0.1`，默认端口为 `17688`，不能配置 LAN 或 public 地址。
- [ ] `/health` 无需鉴权且只返回状态和版本。
- [ ] 另外三个端点必须鉴权；`/v1/models` 返回 Key 权限与当前可路由映射的交集。
- [ ] 候选依次过滤 Key 授权、账号启用、凭据、映射、健康和适用额度。
- [ ] 排序固定为账号用户排序升序、新鲜额度优先、适用窗口最低剩余比例降序、最近使用时间升序，并使用稳定 ID 收尾。
- [ ] 每个逻辑请求最多尝试三个不同账号。
- [ ] 流式首字节前允许按错误类别切换；输出首字节后固定账号。
- [ ] OAuth `401/403` 只刷新并重试一次；第三方 `401/403` 直接授权失效。
- [ ] 请求或模型类其他 `4xx` 不降健康度且不切换。
- [ ] `429` 遵循 `Retry-After`，缺失时从 60 秒指数冷却，最长 15 分钟。
- [ ] 网络或 `5xx` 连续三次后熔断 60 秒，最长 15 分钟；到期只允许一个探测。
- [ ] 候选耗尽返回 `no_available_upstream`，不泄露候选详情。

## Verification Steps

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::router`。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::runtime::http`。
- [ ] 使用本机 mock upstream 覆盖过滤、排序、最多三次尝试和首字节门禁。
- [ ] 用假时钟覆盖 `Retry-After`、冷却递增、熔断、15 分钟上限和单探测。
- [ ] loopback 验证四个端点、Bearer 状态、Key 立即失效及 Models 交集。
- [ ] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`。

## Out of Scope

不接入应用自动启动、Tauri IPC、持久请求日志或前端。
