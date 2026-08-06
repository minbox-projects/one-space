# 03 - TypeScript Typed IPC 原子创建契约

- task_id: `typescript-typed-ipc`
- order: `03`
- blocked_by: `tauri-command-registration`
- source_plan: `../plan.md`
- source_plan_digest: `e8b89e919845f40f6d6d49ba0f3d16866c91d9e0cba261cc610a4b01a5976187`
- write_scope: `src/lib/aiRoutingGateway.ts, src/lib/aiRoutingGateway.test.ts（非穷举，仅限 typed IPC 类型、facade 及其测试）`

## 预期结果

前端获得唯一的 typed IPC 原子创建入口，以固定 `{ input }` camelCase payload 调用新 Tauri command 并返回 `GatewayAccount`，现有 facade 全部保持兼容。

## 实施清单

- [ ] 导出组合创建的连接输入、显式映射项和按公开模型组织的价格项接口，四类价格均表达为 nullable 或 optional 字段。
- [ ] 新增 `aiRoutingGatewayAccountCreateApiKeyWithConfiguration` facade，固定 invoke 命令名和 `{ input }` 参数包装，并通过既有错误归一化路径返回 `GatewayAccount`。
- [ ] 保留并继续导出旧创建、账号更新、映射保存和价格保存 facade，不改其签名或命令名。
- [ ] 添加 facade 单测，完整断言连接字段、映射项、`publicModelId` 和四类价格的 camelCase payload 与返回值。
- [ ] 添加兼容断言，确认旧 account/mapping/price facade 仍按原命令和参数调用。

## 验收标准

- [ ] 新 facade 仅调用 `ai_routing_gateway_account_create_api_key_with_configuration` 一次，并发送精确的 `{ input }` 结构。
- [ ] 映射和价格类型与 Rust DTO 逐字段对应，不要求组件直接使用 `invoke` 或自行拼装命令。
- [ ] 调用成功返回 `GatewayAccount`，失败继续抛出既有归一化 `AiRoutingGatewayError`。
- [ ] 所有既有 typed facade 的导出、命令名称和参数形状均未回归。

## 验证步骤

- [ ] 运行 `npm test -- src/lib/aiRoutingGateway.test.ts`，预期新原子 facade 与旧 facade 契约测试全部通过。
- [ ] 运行 `npm run build`，预期 TypeScript 契约可被应用正常类型检查和构建。

## 范围外事项

不修改 React 组件、Tauri command 或 Rust 领域代码；不在 facade 中编排旧创建、映射保存或价格保存调用。
