# 04 - 构建账号列表卡片

- task_id: `gateway-account-list-cards`
- order: `04`
- blocked_by: `gateway-account-typed-facade`
- source_plan: `../plan.md`
- source_plan_digest: `6f7f72cf85831fcd249370dc3a8fdcf7ecc5d3981bcc238427b969a3642dbbe1`
- write_scope: `src/components/AiRoutingGateway/AccountList*、对应独立组件测试`

## 预期结果
提供可复用、可独立测试的账号列表卡片组件，完整展示认证类型、地址、映射摘要和禁用状态，并让卡片导航与启停、排序、删除、编辑等局部操作互不干扰。

## 实施清单
- [ ] 新建账号列表与卡片组件，沿用现有 Tailwind、图标、按钮和可访问名称风格。
- [ ] 展示账号名、API Key/OAuth 标签、API 地址和模型映射；OAuth 地址固定为 `-`。
- [ ] 映射最多展示前三项，溢出时显示正确的剩余数量，并明确标记禁用映射。
- [ ] 卡片主体与编辑按钮触发详情回调；启停、排序、删除等控件阻止事件冒泡并调用独立操作回调。
- [ ] 用独立组件测试覆盖 API Key/OAuth、映射截断、禁用状态、导航和局部操作事件隔离。

## 验收标准
- [ ] API Key 与 OAuth 卡片均显示正确认证类型，OAuth 地址始终为 `-`。
- [ ] 三项以内全部展示，超过三项时仅展示前三项及正确的“其余 N 项”。
- [ ] 禁用映射和禁用账号具有明确且可访问的状态标识。
- [ ] 点击主体或编辑按钮进入详情；点击启停、排序或删除不会误触发详情导航。
- [ ] 卡片组件不请求详情数据、不接触 API Key 明文，也不耦合 AI Environments DTO 或数据源。

## 验证步骤
- [ ] 运行账号列表卡片独立组件测试并确认所有展示与事件断言通过。
- [ ] 运行相关 TypeScript 编译检查，确认组件只依赖 typed facade 的公开列表契约。

## 范围外事项
不修改账号页总状态协调、不实现详情表单，也不连接 AI Environments 的组件或数据。
