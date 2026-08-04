# 06 - 集成账号列表与详情状态

- task_id: `gateway-account-page-integration`
- order: `06`
- blocked_by: `gateway-account-list-cards, gateway-account-detail-form`
- source_plan: `../plan.md`
- source_plan_digest: `6f7f72cf85831fcd249370dc3a8fdcf7ecc5d3981bcc238427b969a3642dbbe1`
- write_scope: `src/components/AiRoutingGateway/index.tsx、账号页状态协调组件、src/components/AiRoutingGateway/AiRoutingGateway.test.tsx、src/i18n.ts`

## 预期结果
账号池完成内部 list/detail 页面集成：详情加载、原子保存、筛选恢复、脏状态确认、成功反馈、失败保留和永久删除均按规格工作，同时保留既有启停、排序及其他页签行为。

## 实施清单
- [ ] 从 `index.tsx` 拆出账号页状态协调，维护 `viewMode`、当前账号 ID、详情加载、筛选、保存、删除和成功提示状态。
- [ ] 接入列表卡片和详情表单；新增仅允许 API Key，现有 OAuth 进入只读详情。
- [ ] 进入详情时调用专用详情 facade；保存时只调用一次原子保存 facade，成功后清除敏感草稿、刷新 Bootstrap、恢复筛选并返回列表。
- [ ] 保存或加载失败时保留当前页面和草稿，展示不包含明文的通用错误及重试入口。
- [ ] 在返回、切换卡片、切换模式和删除前处理脏状态确认；确认离开后清除详情草稿。
- [ ] 列表和编辑页永久删除均复用现有一次性确认令牌流程；启停和排序继续使用既有窄命令。
- [ ] 补充所需中英文翻译键和无障碍标签，不新增 URL 路由或深链接。
- [ ] 扩展页面级测试，覆盖导航、筛选恢复、脏状态、保存成功/失败、字段错误、删除令牌及旧保存命令不再被调用。

## 验收标准
- [ ] 新增、卡片主体和编辑按钮进入正确详情模式，OAuth 始终只读。
- [ ] 返回、切换详情或删除脏草稿前要求确认；取消时保留页面，确认后清除包含密钥的草稿。
- [ ] 保存成功只发出一次原子保存调用，随后刷新并返回列表、恢复原筛选且显示成功提示。
- [ ] 保存失败保留全部表单输入，并显示错误汇总、字段错误和首个错误定位。
- [ ] 列表与详情删除均先获取并消费一次性确认令牌；失败时保留当前状态。
- [ ] 账号页不再调用独立账号更新、映射保存和价格保存命令完成详情保存。
- [ ] 其他网关页签、账号启停、排序及事件刷新行为无回归，且没有新增路由或 AI Environments 数据依赖。

## 验证步骤
- [ ] 运行 `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`，确认完整页面交互通过。
- [ ] 运行账号组件相关测试，确认独立组件与集成状态契约一致。
- [ ] 运行 `npm run lint` 和 `npm run build`，确认翻译、类型和组件装配通过。

## 范围外事项
不恢复 OAuth 授权，不新增 URL 路由，不合并账号池与 AI Environments 数据，也不删除旧后端兼容命令。
