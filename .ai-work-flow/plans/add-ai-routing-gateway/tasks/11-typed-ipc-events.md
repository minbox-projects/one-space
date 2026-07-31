# 11 - 接入 Typed IPC、事件与 Tauri 注册

- task_id: `11-typed-ipc-events`
- order: `11`
- blocked_by: `10-http-runtime-lifecycle`
- source_plan: `../plan.md`
- source_plan_digest: `385b139e1c25f8e8112982ed63ac3c3f0282be095c8322006f82f45d9070cf6d`
- write_scope: `src-tauri/src/ai_routing_gateway/commands.rs、src-tauri/src/lib.rs、src/lib/aiRoutingGateway.ts、对应 Rust/Tauri mock 与前端 facade 测试`

## Outcome

所有网关管理能力均通过独立 `ai_routing_gateway_*` 命名空间和单一 TypeScript facade 暴露，事件和 DTO 不包含敏感字段。

## Implementation Checklist

- [ ] 为运行状态、设置、账号、分组、标签、OAuth、额度、模型、Key、日志、价格、统计和维护注册独立命令。
- [ ] 为所有写命令定义明确输入 DTO、后端校验和事务调用；输出 DTO 仅包含前端需要的非敏感字段。
- [ ] 注册运行状态、OAuth 会话、额度/账号状态和后台维护进度事件。
- [ ] 在 `src/lib/aiRoutingGateway.ts` 集中定义命令字符串、事件名、输入输出类型、错误映射和订阅清理。
- [ ] 保证 Key 明文只存在于创建或重新生成的一次性成功 DTO，凭据读取永不返回明文。
- [ ] 将命令注册接入 Tauri，但保持 Protocol Router 命令及状态不变。

## Acceptance Criteria

- [ ] 计划规定的所有管理域均有 typed invoke 方法，前端其他位置不需要散布命令字符串。
- [ ] 命令使用独立命名空间，不引用 Protocol Router 的配置类型、状态、监听器或命令。
- [ ] 写命令的无效输入返回稳定类型化错误，事务失败不留下部分状态。
- [ ] 事件负载仅包含稳定实体 ID 和非敏感状态，不包含 Token、code、PKCE verifier、API Key 或请求正文。
- [ ] Tauri mock 测试覆盖命令输入输出、错误映射、事件订阅和取消订阅。
- [ ] 一次性 Key 明文不会进入持久前端状态、事件或日志。

## Verification Steps

- [ ] 执行 Rust 命令注册和 DTO 脱敏测试。
- [ ] 执行 `aiRoutingGateway` facade 的 Vitest/Tauri mock 测试。
- [ ] 执行 TypeScript 类型检查、`cargo test` 和 `cargo check`。

## Out of Scope

不在 facade 中引入全局状态库，不实现页面布局，也不修改 Protocol Router 内部实现。
