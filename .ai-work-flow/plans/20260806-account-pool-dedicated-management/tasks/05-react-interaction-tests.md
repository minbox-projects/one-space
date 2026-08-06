# 05 - 账号池跨状态交互测试

- task_id: `react-interaction-tests`
- order: `05`
- blocked_by: `account-list-detail-ui, rust-domain-atomic-creation`
- source_plan: `../plan.md`
- source_plan_digest: `e8b89e919845f40f6d6d49ba0f3d16866c91d9e0cba261cc610a4b01a5976187`
- write_scope: `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx, src/test/mocks/tauri.ts（非穷举，仅限 React 交互测试及必要的既有 Tauri mock 扩展）`

## 预期结果

自动化 React 测试覆盖账号卡片、list/detail/create/edit 状态、原子创建 payload、成功与失败状态、OAuth 只读和旧 API Key 编辑兼容，形成前端到 typed IPC 边界的回归保护。

## 实施清单

- [ ] 更新旧折叠详情断言，使其验证卡片信息、列表不内嵌详情、卡片主体/编辑进入详情及返回列表。
- [ ] 覆盖账号名称、认证标签、API 地址、映射列表，以及筛选、排序、启停和删除等既有管理动作。
- [ ] 覆盖创建页基于官方模型生成同名启用映射和四类空价格，并断言用户覆盖后的完整原子 payload。
- [ ] 断言新增保存只调用新 facade，不调用旧创建、映射保存或价格保存。
- [ ] 覆盖创建成功后刷新并返回列表、清理敏感值，以及失败后停留详情并保留连接字段、API Key、映射和价格。
- [ ] 覆盖 API Key 编辑仍调用旧账号更新、映射保存和价格保存路径。
- [ ] 覆盖 OAuth 详情展示已有映射和价格，但不出现可写控件且不触发写调用。
- [ ] 仅在现有 mock 无法表达新命令时最小扩展 `src/test/mocks/tauri.ts`，保持其他测试默认行为。

## 验收标准

- [ ] React 定向测试对卡片和所有 list/detail/create/edit 转换提供稳定断言。
- [ ] 原子新增测试逐字段验证连接信息、最终映射和四类 camelCase 价格，并证明无旧写命令拼接。
- [ ] 成功与失败分支分别证明刷新返回/敏感值清理和表单完整保留。
- [ ] OAuth 只读与 API Key 旧编辑路径均有明确的正向展示和负向调用断言。

## 验证步骤

- [ ] 运行 `npm test -- src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`，预期账号池交互测试全部通过。
- [ ] 运行 `npm test -- src/lib/aiRoutingGateway.test.ts src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`，预期 facade 与组件边界契约同时通过。

## 范围外事项

不修改生产组件、typed facade 或 Rust 源码；不通过降低断言、跳过测试或扩大 mock 默认成功行为来掩盖产品缺陷。
