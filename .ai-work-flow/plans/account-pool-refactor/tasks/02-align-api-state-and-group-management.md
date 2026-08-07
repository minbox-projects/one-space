# 02 - 对齐 TypeScript API、账号池状态与分组管理

- task_id: `align-api-state-and-group-management`
- order: `02`
- blocked_by: `implement-account-pool-backend`
- source_plan: `../plan.md`
- source_plan_digest: `5580666a0b5285182d47ad850a271e4f8faf8cec0b380701a079849ff084ea1d`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src/lib/aiRoutingGateway.ts`
  - `src/components/AiRoutingGateway/index.tsx`
  - `src/i18n.ts`

## 预期结果

同步 TypeScript DTO、完整创建输入、分组重命名及按账号 ID 集合执行的批量命令封装，并将活动分组、搜索、可见账号和选择集合限定在当前分组；接入默认组优先且无‘全部账号’的横向 tabs 与分组管理能力。验收以命令名、camelCase 参数、Serde 映射和错误类型与 Rust 一致，默认组不可重命名或删除，分组生命周期正确，刷新、切组或删除后能回退有效分组并清理失效选择为准。

## 实施清单

- [ ] 扩展 `CreateApiKeyAccountWithConfigurationInput`，加入 `groupId`、`tags` 和 `quotaThresholdOverridePercent`，并与后端完整创建 DTO 的可选值、空值及 camelCase 规则逐项一致。
- [ ] 在 `src/lib/aiRoutingGateway.ts` 增加分组重命名、批量禁用、批量删除确认及批量删除 facade；输入必须是显式 `accountIds` 集合，返回类型和 `AiRoutingGatewayError` 行为与 Rust 命令一致，同时保持既有单账号创建、更新、移动和删除 facade 不变。
- [ ] 在 `AccountsTab` 建立 `activeGroupId`、搜索文本和选择 ID 集合。分组按 `is_default` 固定默认组首位，其余保持后端排序；初次加载选择默认组，不渲染“全部账号”tab。
- [ ] 先按 `group_id === activeGroupId` 限定账号，再以名称、API 地址、认证方式、协议、标签、映射或备注中的可见文本执行搜索；可见账号和全选候选均从这一派生结果产生。
- [ ] 在数据刷新、切换分组、搜索条件变化、分组删除及账号删除后校正状态：活动组不存在时回退默认组；选择集合与当前组且当前搜索可见的账号 ID 求交，不允许陈旧或隐藏 ID 进入后续批量请求。
- [ ] 以横向 tabs 呈现分组并提供管理入口；接入项目现有 Dialog 组件，支持新建、重命名和删除自定义组。默认组不显示可执行的重命名/删除控件，前端仍处理后端拒绝受限操作的错误。
- [ ] 分组创建、重命名、删除仅在命令成功后重新 bootstrap；失败时保留可重试输入并显示错误。删除当前组成功后刷新数据并回退有效默认组，删除迁移由后端负责，前端不得本地伪造迁移结果。
- [ ] 在 `src/i18n.ts` 的既有 AI 路由网关命名空间补齐中英文分组 tabs、管理、新建、重命名、删除、搜索、选择清理及错误文案，复用通用按钮文案，不改变其他模块翻译键。

## 验收标准

- [ ] 新 facade 调用的命令名、`input` 包装、`groupId`/`accountIds`/确认令牌等 camelCase 参数与 Rust Serde 契约逐项一致，失败统一抛出 `AiRoutingGatewayError`。
- [ ] 默认组始终位于第一个 tab 且初始激活；页面不存在“全部账号”tab，默认组没有可执行的重命名或删除入口。
- [ ] 搜索、可见账号、全选候选及选择集合始终受当前分组约束；切组或搜索隐藏账号后，失效选择不会进入请求。
- [ ] 自定义组可新建、重命名和删除；命令失败不显示虚假成功状态，删除当前组后重新加载并回退默认组。
- [ ] 刷新后活动组不存在时自动恢复有效默认组，已删除、跨组或搜索隐藏账号的选择被清理。
- [ ] 既有账号编辑、排序、启停和单账号删除调用形状保持兼容，首页和其他网关 tabs 不受账号池分组状态影响。

## 验证步骤

- [ ] 运行 `npm test -- --run src/lib/aiRoutingGateway.test.ts`，预期完整创建、分组重命名和批量命令的命令名及参数断言通过，既有 facade 回归通过。
- [ ] 运行 `npm test -- --run src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`，预期默认组优先、无全部账号 tab、组内搜索、分组生命周期、失败状态和有效组回退测试通过。
- [ ] 运行 `npm run build`，预期 TypeScript DTO、React 状态和 i18n 键使用通过类型检查与构建。

## 范围外事项

- 不实现 Rust 数据事务、命令注册或数据库迁移。
- 本任务只建立账号池范围状态、API 和分组管理；纵向账号项的最终视觉结构、批量工具栏及新增视图布局由后续任务完成。
- 不引入 React Router、独立 URL、“全部账号”tab、跨组搜索或整组批量命令。
