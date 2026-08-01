# 09 - 接入 IPC 与应用生命周期

- task_id: `ai-routing-ipc-lifecycle`
- order: `09`
- blocked_by: `ai-routing-logging-aggregates`
- source_plan: `../plan.md`
- source_plan_digest: `4fd0f21d03c9fd3060e030ce14e81636333e3060e47dee01c6720c5983f985ce`
- write_scope: `src-tauri/src/ai_routing_gateway/commands.rs；src-tauri/src/ai_routing_gateway/runtime/{mod.rs,lifecycle.rs,events.rs}；src-tauri/src/ai_routing_gateway/types/ipc.rs；src-tauri/src/app_runtime/run_app.rs；src-tauri/src/app_runtime/ 中既有 Tauri command wiring 边界；对应 Rust 测试；禁止修改 src-tauri/src/lib.rs`

## Outcome

网关通过独立 Tauri IPC 和事件可完整管理，并在数据库与 Keychain 就绪后自动启动、受控重启及优雅退出。

## Implementation Checklist

- [ ] 建立 `ai_routing_gateway_*` 命令命名空间和明确 DTO。
- [ ] 注册运行状态、设置、账号、OAuth、额度、模型、Key、日志、价格、统计和维护命令。
- [ ] 发布运行状态、OAuth、额度/账号和维护进度事件。
- [ ] 实现幂等启动、端口预检、停止、受控重启和退出排空。
- [ ] 将数据库初始化、Keychain 检查、服务启动和退出按固定顺序接入 `run_app.rs`。
- [ ] 实现数据库失败、Keychain 锁定和端口冲突的稳定运行状态。

## Acceptance Criteria

- [ ] 所有写命令都有后端输入校验和事务边界。
- [ ] IPC DTO 和事件不携带 token、code、PKCE verifier、API Key、验证材料或请求正文。
- [ ] 数据库迁移先于 Keychain 检查，Keychain 就绪先于 HTTP 服务自动启动。
- [ ] 数据库失败、Keychain 锁定或端口冲突时服务保持停止，且不循环抢占端口。
- [ ] 运行中修改端口先预检，再停止接入、等待排空并重新绑定；失败时返回稳定状态。
- [ ] 完全退出先拒绝新请求，再等待已完成请求日志事务；未完成流记录为取消或中断。
- [ ] 排空具有明确上限，超时不会伪报成功。
- [ ] `src-tauri/src/lib.rs` 不产生本任务差异。
- [ ] 不引用 Protocol Router 的状态、监听器、类型或命令命名空间。

## Verification Steps

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::commands`。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::runtime`。
- [ ] 用本机占用端口测试验证冲突时不重试抢占。
- [ ] 测试数据库失败、Keychain 锁定、自动启动、幂等启动、改端口和完全退出顺序。
- [ ] 捕获事件和 IPC 输出并执行敏感标记扫描。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml`。
- [ ] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`。

## Out of Scope

不修改 `src-tauri/src/lib.rs`，不实现前端 facade、导航或页面。
