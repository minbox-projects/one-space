# 纯验证 Codex OAuth 跨层行为与回归结果

## 预期结果

依赖任务 01 至 05；作为纯验证任务运行并按验证边界补齐测试或报告，覆盖 Rust 协议与凭据生命周期、Tauri 注册与 typed IPC、敏感数据边界、React 登录状态机及 API Key/Gmail 回归，并只读复验三份导航索引。该任务不含任何生产实现或导航索引写权限；发现不一致时必须阻断并回交对应实施任务。

## 实施清单

- [ ] 补齐 Rust OAuth 协议测试：provider 参数、PKCE S256、随机 loopback/state/nonce、自动与手动 callback 共用校验、TTL、取消、错误和重放。
- [ ] 补齐 token/OIDC 测试：请求参数、交换失败、畸形响应、可信 JWKS 签名、exp/iss/aud/nonce、无可信 JWKS 降级以及可靠主体缺失拒绝。
- [ ] 补齐账号生命周期测试：`chatgpt_account_id` 去重、workspace 隔离、`sub` 回退、AES-GCM 明文无泄漏、事务失败回滚、refresh rotation 和退出登录本地优先语义。
- [ ] 补齐 runtime 测试：到期前刷新、授权失败最多一次刷新与一次原请求重试、临时错误有限退避、永久失败重新授权标记和候选剔除。
- [ ] 补齐 Tauri/IPC 测试：command 注册清单、Rust/TypeScript DTO 参数一致、listener 失败手动回退、状态事件终态以及所有公开 payload 无敏感字段。
- [ ] 补齐 React 测试：OAuth/API Key 并列入口、等待、取消、超时、错误、手动完整 callback、成功 bootstrap 刷新、重新授权、退出登录和 OAuth 连接字段只读。
- [ ] 执行定向、全量、lint 和构建回归，确认 API Key 创建/编辑/路由、Gmail OAuth 及网关 Bootstrap 行为未改变；生产实现失败时阻断并回交对应实施任务，不在本任务中修复。
- [ ] 以只读方式复验 `.ai-work-flow/index/feature-navigation.md`、`.ai-work-flow/index/backend-navigation.md` 与 `.ai-work-flow/index/frontend-navigation.md`，逐项对照最终四个 OAuth commands、`run_app.rs` 注册入口、`oauth.rs` 领域入口、TypeScript OAuth facade 和 OAuth 事件订阅；不一致时阻断，不越界修改索引。

## 验收标准

- [ ] Rust 测试可判定地覆盖 callback 校验先于 token 交换、可信 OIDC 验证、降级可见性、稳定身份、加密落库、rotation、刷新重试和退出登录。
- [ ] Tauri command 注册和 typed facade 的命令名、参数名、序列化字段及事件状态完全一致，敏感字段负向断言通过。
- [ ] React 测试覆盖完整登录状态机、手动 callback、成功刷新、重新授权和 OAuth 连接字段只读，现有 API Key 交互测试保持通过。
- [ ] `cargo test`、前端定向测试、`npm test`、`npm run lint` 和 `npm run build` 全部成功。
- [ ] Gmail OAuth、API Key 与 gateway bootstrap 的既有测试无回归，代码与测试日志不含 token、authorization code、PKCE verifier 或完整 callback URL。
- [ ] 三份导航索引经只读复验与最终代码入口一致；任务 write scope 不包含 `.ai-work-flow/index/`，任何不一致均阻断验收。

## 验证步骤

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml`，预期全部 Rust 单元与集成测试通过。
- [ ] 运行 `npm test -- --run src/lib/aiRoutingGateway.test.ts src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`，预期 typed IPC 与 React OAuth 定向测试通过。
- [ ] 运行 `npm test`，预期包括 Gmail OAuth、API Key 和网关 Bootstrap 在内的前端全量测试通过。
- [ ] 运行 `npm run lint`，预期 ESLint 无错误。
- [ ] 运行 `npm run build`，预期 TypeScript 与生产构建成功。
- [ ] 审查测试输出和失败快照，预期不出现 access/refresh/id token、authorization code、PKCE verifier 或完整 callback URL。
- [ ] 只读逐项核对三份导航索引与最终四个 OAuth commands、`run_app.rs` 注册入口、`oauth.rs` 领域入口、`aiRoutingGateway.ts` facade 及 OAuth 事件订阅，预期全部一致；任一不一致立即阻断且不修改索引。

## 范围外事项

- 不新增已确认六项任务以外的产品能力、迁移或界面重构。
- 不修改 `oauth.rs`、`accounts.rs`、`runtime.rs`、`commands/mod.rs`、`run_app.rs`、前端生产实现或任何导航索引。
- 不以降低 OIDC 校验、明文保存凭据或增加无限重试的方式修复测试。
