# 06 - 跨层最终回归与发布门禁

- task_id: `final-cross-layer-regression`
- order: `06`
- blocked_by: `tauri-command-registration, typescript-typed-ipc, react-interaction-tests`
- source_plan: `../plan.md`
- source_plan_digest: `e8b89e919845f40f6d6d49ba0f3d16866c91d9e0cba261cc610a4b01a5976187`
- write_scope: `无（仅执行跨层验证并检查限定范围；失败应交回拥有对应核心文件的前置 task）`

## 预期结果

Rust、Tauri 注册、typed IPC、React 交互、前端质量门禁和全量 Rust 测试全部通过，并确认 schema、migration 与冻结范围外模块没有因本功能发生改动。

## 实施清单

- [ ] 依次运行 facade、React、Rust accounts、Rust pricing 和命令注册定向测试，记录所有结果并先定位任何失败所属层。
- [ ] 运行前端完整测试、lint 和生产构建，确认新旧账号池流程均未造成全局回归。
- [ ] 运行 Rust 全量测试，确认原子事务 helper 和新命令未影响既有网关能力。
- [ ] 检查 `schema_v1.sql` 至 `schema_v4.sql`、shared SQLite migration、`AiEnvironments`、路由和 OAuth 登录相关文件未因本功能被修改。
- [ ] 按人工验收路径检查 API Key 新增、卡片展示、详情切换、OAuth 只读及既有 API Key 编辑行为。
- [ ] 若验证失败，报告失败命令、最小复现和对应前置 task，不在本 task 越权修改其核心文件。

## 验收标准

- [ ] 所有冻结 plan 列出的定向命令与完整回归命令退出码均为 0。
- [ ] 新增 API Key 仅使用原子命令；旧创建、账号更新、映射保存和价格保存接口仍存在且既有编辑路径可用。
- [ ] OAuth 前端无映射/价格写入口，后端 mapping 和 price 两条防线均有效。
- [ ] schema、migration 及范围外模块无功能性改动，不需要数据迁移或回填。
- [ ] 人工验收观察与自动化断言一致，不存在半成品账号、明文密钥或失败后表单丢失。

## 验证步骤

- [ ] 运行 `npm test -- src/lib/aiRoutingGateway.test.ts`，预期通过。
- [ ] 运行 `npm test -- src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`，预期通过。
- [ ] 运行 `cargo test ai_routing_gateway::accounts --manifest-path src-tauri/Cargo.toml`，预期通过。
- [ ] 运行 `cargo test ai_routing_gateway::pricing --manifest-path src-tauri/Cargo.toml`，预期通过。
- [ ] 运行 `cargo test ai_routing_gateway_commands_are_registered_once_in_the_isolated_block --manifest-path src-tauri/Cargo.toml`，预期通过。
- [ ] 运行 `npm test && npm run lint && npm run build && cargo test --manifest-path src-tauri/Cargo.toml`，预期完整回归全部通过。

## 范围外事项

本 task 不修复产品源码、不修改测试、不提交 Git；不接受 schema/migration 改动，不新增 URL、深链接、OAuth 登录或官方模型/价格管理能力。任何失败修复必须回到拥有对应文件的前置 task。
