# 04 - 补齐前后端测试并完成账号池回归验证

- task_id: `verify-account-pool-refactor`
- order: `04`
- blocked_by: `implement-account-pool-backend, align-api-state-and-group-management, build-account-pool-interface`
- source_plan: `../plan.md`
- source_plan_digest: `5580666a0b5285182d47ad850a271e4f8faf8cec0b380701a079849ff084ea1d`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src-tauri/src/ai_routing_gateway/accounts.rs`
  - `src-tauri/src/ai_routing_gateway/commands/mod.rs`
  - `src-tauri/src/app_runtime/run_app.rs`
  - `src-tauri/src/shared_sqlite/mod.rs`
  - `src-tauri/src/shared_sqlite/migrations.rs`
  - `src/lib/aiRoutingGateway.test.ts`
  - `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`

## 预期结果

完善 Rust、Tauri 命令、TypeScript API 和 React 组件测试，覆盖完整创建与回滚、分组保护与迁移、命令参数、组内搜索和选择、纵向列表、批量成功失败与取消、新增视图往返及单账号操作回归；执行相关 Vitest、Cargo 测试、类型检查、lint，并在窄宽度下验证 tabs、列表、工具栏和弹窗无重叠。验收以规格规定的成功与失败路径全部受测且相关回归通过为准。

## 实施清单

- [x] 在 `accounts.rs` 的现有内联测试模式中覆盖完整创建的分组、标签、阈值、备注、连接、映射和价格读回，并逐项构造非法分组、越界阈值、未知映射和非法价格，断言事务无残留。
- [x] 增加分组重命名、默认组重命名/删除保护、自定义组删除迁移及事务失败回滚测试；验证已有账号及默认组在可选迁移后保持兼容。
- [x] 增加批量禁用和批量删除数据层测试，覆盖空/未知/混合目标、全量预校验、已禁用幂等、有效确认、取消、过期/复用/集合不匹配确认，以及失败不产生部分写入。
- [x] 在命令模块测试中覆盖新 DTO 的 camelCase 反序列化、错误分类、账号事件/响应和批量确认语义；以注册清单测试或等价静态断言确认 `run_app.rs` 暴露全部新命令。
- [x] 若任务 01 新增 schema v5，在 `shared_sqlite` 现有迁移测试中覆盖从 v4 升级、重复启动幂等、已有账号/默认组保留和约束生效；若未新增迁移，不为不存在的 schema 制造测试或生产改动。
- [x] 扩展 `src/lib/aiRoutingGateway.test.ts`，断言完整创建、分组重命名、批量禁用、批量删除确认和批量删除的精确命令名、`input` 包装、camelCase 参数及错误归一化；保留单账号 facade 回归断言。
- [x] 扩展 React fixture，使其至少包含默认组、自定义组、同组可见/隐藏账号、跨组账号、启用/禁用账号及完整展示字段；避免测试只覆盖单账号快乐路径。
- [x] 增加组件测试：默认组首位且无全部账号、组内搜索、分组新建/重命名/删除与失败回退、纵向列表字段、空状态、切组/刷新后的选择清理及全选仅覆盖当前搜索结果。
- [x] 增加批量交互测试：无选择不可执行、禁用包含已禁用账号、删除数量确认、取消零请求、成功刷新并清空选择、失败保留选择，并断言发送 ID 不包含跨组或隐藏账号。
- [x] 增加创建视图测试：模块内往返不改变 URL、取消零写入、一次请求携带全部字段、成功回列表刷新、失败保留输入；回归单账号编辑、排序、启停、删除、映射和价格操作。
- [x] 执行定向测试、相关全量回归、类型检查和 lint；记录并修复由本重构引入的失败，不以放宽断言、跳过测试或删除既有覆盖作为通过手段。
- [x] 在 1440x1000 和 390x844 通过真实 Playwright fixture 验证渲染：8 个分组 tabs 均实际滚入并点击可达末尾，分组弹层记录 bounding box、视口 containment、关键控件 visible/enabled 和 elementFromPoint 未遮挡结果；默认组无重命名/删除入口，7 个自定义组均有可操作入口；长名称/API 地址/标签/映射、批量工具栏、创建表单及确认弹窗结果见 [`std-003-evidence.md`](../std-003-evidence.md)。

## 验收标准

- [x] Rust 测试可判定完整创建的原子性、字段读回、分组保护/迁移、批量全量校验、禁用幂等和删除确认集合绑定语义。
- [x] Tauri/TypeScript 契约测试同时约束命令名、camelCase 参数、返回类型和错误类型，新增接口在两端不存在字段漂移。
- [x] React 测试覆盖规格中的分组、搜索、选择、纵向列表、批量成功/失败/取消、新增视图成功/失败/返回，以及既有单账号操作。
- [x] 测试明确证明搜索、全选和批量请求不会跨组或包含隐藏账号，刷新、切组和删除后不会保留失效选择。
- [x] 相关 Vitest、Cargo 测试、TypeScript 构建和 ESLint 全部通过；无新增跳过、仅运行标记或脆弱的实现细节快照。
- [x] 桌面与移动实测中，`documentWidth/clientWidth`、`horizontalOverflow`、browserErrors 和持久 JSON/脚本证据见 [`std-003-evidence.md`](../std-003-evidence.md)；JSON 的 `assertions` 分别持久记录 `group-tabs-horizontal-reachability`、`group-dialog-viewport-controls`、`default-group-protection` 和 `custom-group-actions`。

## 验证步骤

- [x] 运行 `npm test -- --run src/lib/aiRoutingGateway.test.ts src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`，预期账号池 API 与组件用例全部通过。
- [x] 运行 `npm test -- --run`，预期前端相关全量回归通过。
- [x] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway`，预期 AI 路由网关数据层、命令层和既有相关用例通过。
- [x] 运行 `cargo test --manifest-path src-tauri/Cargo.toml shared_sqlite`，预期数据库初始化及迁移回归通过。
- [x] 运行 `cargo test --manifest-path src-tauri/Cargo.toml`，预期 Rust 全量测试通过。
- [x] 运行 `npm run build` 和 `npm run lint`，预期 TypeScript、Vite 构建和 ESLint 均无错误。
- [x] 运行 `npm run dev -- --host 127.0.0.1 --port 4174` 后以 1440x1000 和 390x844 打开账号池并保存实际 Playwright 检查结果；由于 4173 已由其他 worktree 占用，本轮命令与 JSON 均使用 4174，证据见 [`std-003-evidence.md`](../std-003-evidence.md)。

## 本轮可审计证据

2026-08-07 本轮 `coding.fix_direct` 的真实渲染、交互结果、视口尺寸、tabs 实际滚动、弹层几何与控件遮挡判断、browserErrors、可重放脚本、JSON 报告、前端检查命令及 Rust 未重跑原因统一记录在 [`../std-003-evidence.md`](../std-003-evidence.md)，原始自动化结果在 [`../std-003-playwright-report.json`](../std-003-playwright-report.json)。上述 `[x]` 状态以这些仓库内持久证据为准。

## 范围外事项

- 不增加规格之外的产品能力，不借测试任务重构无关模块或改变既有业务语义。
- 不通过修改 `spec.md`、`plan.md`、删除既有测试、降低断言强度或跳过检查来获得通过。
- 不承担发布、真实生产数据迁移或应用版本回滚；发布前数据库副本验证由发布流程执行。
