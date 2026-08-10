# 修复并接通 Codex OAuth Tauri 与 Typed IPC

## 预期结果

依赖任务 01、02、03；基于尚未整合的提交 83e0a36dfe7509cf51379a5d6e1e589ef6509cc9 修复 SPEC-OAUTH-001、SPEC-OAUTH-002、SPEC-OAUTH-003 与 standards-index-sync-001，注册开始、完成、取消和退出登录命令，编排 loopback listener 与系统浏览器，接通 Rust DTO、状态事件和 TypeScript facade；独占同步三份导航索引的写入职责。

## 实施清单

- [ ] 将 OAuth store 以生产 Codex provider 配置初始化，并在 `generate_handler!` 注册开始登录、完成手动回调、取消和退出登录 commands。
- [ ] 开始登录时先绑定随机本地端口并创建 session，再启动临时 loopback listener 和系统浏览器；listener 负责消费自动 callback 并汇合到同一后端完成服务。
- [ ] listener 启动或运行失败时发出安全状态并保留未过期 session，允许用户通过手动 command 提交完整 callback；取消、超时和终态错误清理 listener 与 session。
- [ ] 设计 Rust 输入/输出 DTO 与 OAuth 状态事件，仅传递 session ID、非敏感状态、授权 URL、过期信息和成功账号标识；完整 callback URL 只允许作为手动 command 输入并不得回显。
- [ ] 在 `aiRoutingGateway.ts` 增加与 Rust camelCase 序列化一致的 begin、manual complete、cancel、logout facade、结果类型和等待/成功/取消/超时/失败状态事件。
- [ ] 对 command 错误做安全分类，确保日志、Tauri invoke 返回和事件均不包含 token、authorization code、PKCE verifier 或完整 callback URL。
- [ ] 同步 `.ai-work-flow/index/feature-navigation.md`、`.ai-work-flow/index/backend-navigation.md` 与 `.ai-work-flow/index/frontend-navigation.md`，准确记录四个 OAuth commands、`run_app.rs` 注册、`oauth.rs` 领域入口、TypeScript OAuth facade 和 OAuth 事件订阅，且不写入敏感材料。
- [ ] 基于提交 `83e0a36dfe7509cf51379a5d6e1e589ef6509cc9` 的评审事实，继续修复 `SPEC-OAUTH-001`、`SPEC-OAUTH-002`、`SPEC-OAUTH-003` 与 `standards-index-sync-001`。
- [ ] 仅本任务可写入三份导航索引；任务 01-03、05、06 均不得修改这些索引。

## 验收标准

- [ ] 所有 Codex OAuth commands 均可通过 Tauri invoke handler 调用，Rust DTO 字段名与 TypeScript facade 参数完全一致。
- [ ] 自动 loopback 成功会完成登录；listener 失败后同一 session 仍可手动完成；取消、超时和终态错误会释放 listener 并清理会话。
- [ ] begin command 能打开系统浏览器，并在浏览器打开失败时返回可恢复状态而不泄漏或错误消费 session。
- [ ] IPC 结果和 OAuth 事件不包含 access/refresh/id token、authorization code、PKCE verifier 或完整 callback URL。
- [ ] 退出登录 command 调用既有本地清除/禁用生命周期并返回非敏感结果。
- [ ] 三份导航索引准确覆盖四个 OAuth commands、`run_app.rs` 注册入口、`oauth.rs` 领域入口、TypeScript OAuth facade 与 OAuth 事件订阅，且不含 access token、refresh token、id_token、authorization code、PKCE verifier 或完整 callback URL。
- [ ] `SPEC-OAUTH-001`、`SPEC-OAUTH-002`、`SPEC-OAUTH-003` 与 `standards-index-sync-001` 四个 blocking findings 均已修复并可独立复核。

## 验证步骤

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::commands`，预期 command DTO、listener 编排、事件脱敏与退出登录测试通过。
- [ ] 运行 `npm test -- --run src/lib/aiRoutingGateway.test.ts`，预期 facade 命令名、参数序列化、事件类型和敏感字段约束测试通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml`，预期 `run_app.rs` command 注册与 Rust 全量测试通过。
- [ ] 逐项核对三份导航索引与四个 OAuth commands、`run_app.rs`、`oauth.rs`、`aiRoutingGateway.ts` 及 OAuth 事件订阅，预期导航准确且搜索不到任何敏感材料。
- [ ] 按既有评审用例复验四个 blocking findings，预期均不再阻断整合。

## 范围外事项

- 不实现 React 账号池对话框和本地化视觉文案。
- 不通过 IPC 暴露任何可用于重放授权或访问上游的秘密材料。
