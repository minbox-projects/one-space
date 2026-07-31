# 10 - 实现 HTTP Runtime 与服务生命周期

- task_id: `10-http-runtime-lifecycle`
- order: `10`
- blocked_by: `09-routing-health-failover`
- source_plan: `../plan.md`
- source_plan_digest: `037804aa9bfa9cdfc9001966bb673f99116f870c328e29c2f1e5ad7aa4c79d19`
- write_scope: `src-tauri/src/ai_routing_gateway/{runtime.rs,http.rs,tests/http.rs,tests/lifecycle.rs}、src-tauri/src/app_runtime/run_app.rs`

## Outcome

AI 路由网关可作为独立 loopback 服务自动启动、受控重启和优雅停止，并提供 Health、Models、Responses 和 Chat Completions 四个端点。

## Implementation Checklist

- [ ] 实现持有数据库、安全状态、HTTP server、OAuth 会话、额度调度器、维护任务和路由健康的独立 runtime。
- [ ] 仅绑定 `127.0.0.1`，默认端口 `17688`，实现端口预检、幂等启动和明确运行状态。
- [ ] 实现 `/health`、`/v1/models`、`/v1/responses` 和 `/v1/chat/completions`，限制请求头和 JSON 大小。
- [ ] 对三个受保护端点接入 Bearer Key 鉴权、授权和路由；Health 仅返回状态及版本。
- [ ] 实现端口变更时停止接入、等待排空、重新绑定，以及完全退出时完成日志提交的优雅停止。
- [ ] 在数据库或 Keychain 未就绪、端口冲突时保持停止并发布稳定状态，不循环抢占。

## Acceptance Criteria

- [ ] 服务只监听 IPv4 loopback，不接受 LAN 或 public 地址配置。
- [ ] Health 匿名可访问且不泄露账号数量、数据库路径、配置或错误堆栈。
- [ ] 三个受保护端点验证 Bearer Key；认证、权限、模型和上游错误使用兼容 envelope 与稳定机器码。
- [ ] `/v1/models` 仅返回当前 Key 授权范围内且至少有一个可路由账号映射的公开模型。
- [ ] 数据库、Keychain 和端口任一未就绪时服务不启动，状态明确且不发生重试抢占循环。
- [ ] 运行中修改端口会受控排空并重启；失败时报告稳定状态，不留下两个监听器。
- [ ] 完全退出拒绝新请求，已完成请求日志提交，未完成流按取消或中断记录。

## Verification Steps

- [ ] 执行四个端点的 Rust loopback 集成测试，覆盖认证、权限、大小限制和错误 envelope。
- [ ] 执行自动启动、依赖未就绪、端口冲突、受控重启和优雅退出生命周期测试。
- [ ] 执行 `cargo test` 和 `cargo check`，测试不得访问公网。

## Out of Scope

不支持 LAN、TLS、CORS、远程访问、多用户服务或 WebSocket。
