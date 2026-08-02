# 06 - 生命周期接线、诊断脱敏与跨层回归收口

- task_id: `task-06`
- order: `06`
- blocked_by: `task-05`
- source_plan: `../plan.md`
- source_plan_digest: `92f85a7f07acc328e48edf775eae5bfb751f58861b7c1b93de18fb68ed5fd822`
- write_scope: `src-tauri/src/app_runtime/run_app.rs；AI 路由网关跨层集成测试、Protocol Router 回归测试、脱敏审查与工程门禁所需的集成修正`

## Outcome

AI 路由网关按严格依赖顺序自动启动、受控重启并优雅退出，所有 IPC 与事件完成唯一注册接线，Protocol Router 行为保持不变，完整工程门禁通过。

## Implementation Checklist

- [x] 唯一负责修改 `src-tauri/src/app_runtime/run_app.rs`，完成状态托管、IPC 注册、初始化、自动启动、事件发布、端口变更和退出排空。
- [x] 负责跨层集成测试、Protocol Router 回归、全仓敏感信息审查和最终构建门禁。
- [x] 负责确认默认自动启动只在 SQLite、安全、HTTP、日志和生命周期回归全部完成后启用。
- [x] 允许对前述模块进行仅限集成缺陷、脱敏缺口和门禁失败的修正，不新增主体功能或计划外接口。

## Acceptance Criteria

- [x] 启动严格遵循“SQLite 与迁移 → Keychain 检查/创建或锁定 → 设置与端口预检 → HTTP、额度、维护调度器”顺序。
- [x] 数据库失败、Keychain 锁定、端口冲突或 listener 绑定失败均使网关保持停止，发布稳定脱敏状态且不循环重试抢占。
- [x] 启动、停止和重启幂等；运行中改端口执行停止接入、有限排空、释放旧 listener、绑定新端口，失败后保持停止。
- [x] 应用退出时停止新请求，在有上限的排空期完成在途请求和日志事务，未完成流记录为取消或中断。
- [x] 全部 `ai_routing_gateway_*` commands 和 runtime/OAuth/额度账号/维护事件完成唯一注册，前端订阅与后端载荷一致。
- [x] Protocol Router 原有初始化、autostart、status event、端口和退出行为通过回归测试且无行为变化。
- [x] 全仓脱敏审查确认 tracing、IPC、HTTP 错误、SQLite 日志、fixture 和诊断不包含正文、提示词、工具参数、Authorization、Cookie、token 或 API Key 明文。
- [x] 生命周期测试覆盖初始化顺序、自动启动、冲突不重试、锁定、受控改端口、退出排空和日志提交。
- [x] `npm run test`、`npm run lint`、`npm run build`、`cargo test`、`cargo check` 及项目既有构建类检查全部通过且不依赖公网；完整本机签名验证由用户明确豁免，以已通过的 `npm run tauri build -- --no-bundle` 作为替代验证。
- [x] 未启动 Playwright、浏览器自动化、E2E、可见浏览器或视觉验证。

## Verification Steps

- [x] 执行生命周期与 Protocol Router 回归测试，并执行 `npm run test`、`npm run lint`、`npm run build`、`cargo test`、`cargo check` 及项目既有构建类检查，确认全部通过且不依赖公网；用户明确授权跳过本机完整签名验证，替代验证为已通过的 `npm run tauri build -- --no-bundle`。

## Out of Scope

不重命名、迁移、复用或改变 Protocol Router 的命令、状态、listener、配置、统计及数据；不新增主体功能或计划外接口；不启动 Playwright、浏览器自动化、E2E、可见浏览器或视觉验证。

## Verification Evidence

- `cargo test --manifest-path src-tauri/Cargo.toml`：通过，444 passed、2 ignored；包含生命周期、loopback HTTP、排空日志提交、SQLite 迁移/回滚、安全锁定及 Protocol Router 回归。
- `cargo check --manifest-path src-tauri/Cargo.toml`：通过。
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`：通过。
- `npm run test`：通过，32 files、280 tests。
- `npm run lint`：通过，0 errors；保留 386 条既有 warnings。
- `npm run build`：通过。
- `npm run tauri build -- --no-bundle`：通过，本地 release application 构建成功。
- 本机签名验证豁免：用户明确授权本机不配置 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`，并跳过完整签名 `npm run tauri build` 的当前验收；不得据此声称签名通过。此前该命令在 release 编译及 `.app`、`.dmg`、updater archive 打包后因缺少签名私钥返回失败，当前以已通过的 `npm run tauri build -- --no-bundle`、Rust/npm 全量验证作为本机替代验收。
- `npm run check:cli-matrix` 未执行：脚本访问公网，按本任务约束排除。
- 未启动 Playwright、浏览器自动化、E2E、可见浏览器或视觉验证。
