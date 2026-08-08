# 03 - 网关密钥分组表格与弹框工作流

- plan_id: `ai-routing-gateway-key-refactor`
- plan_digest: `5b4fb2aa88f441a06603231eb7bda9c0aae2158accd3764d818593e13a25fc22`
- preview_revision: `2`
- task_id: `gateway-key-management-ui`
- order: `03`
- blocked_by: `gateway-key-provider-conversion`
- source_plan: `../plan.md`
- source_plan_digest: `5b4fb2aa88f441a06603231eb7bda9c0aae2158accd3764d818593e13a25fc22`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src/components/AiRoutingGateway/index.tsx`
  - `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`
  - `src/components/AiRoutingGateway/`
  - `src/i18n.ts`
  - `src/i18n.test.ts`

## 预期结果

将 KeysTab 改造为 SSH 式展示分组 tabs 和管理弹框，实现创建编辑、一次性密钥结果、严格列顺序表格、文本与状态组合筛选、带 tooltip 的图标操作及多工具转换弹框，并补齐本地化文案。前端测试和界面检查验证脱敏及费用不可计算显示、窄宽布局、加载错误空态、已转换禁选、统一激活默认关闭、busy 防重、错误重试和操作后分组、列表及可转换工具刷新。

## 范围与依赖

- 依赖 `gateway-key-provider-conversion` 提供完整 typed wrapper、后端计算状态、脱敏列表、一次性明文和转换能力。
- 本任务只消费后端业务真值；页面不得自行推导密钥状态、费用可计算性、可转换工具、base URL 或工具专属服务商字段。
- `src/components/AiRoutingGateway/` 仅允许新增当前 KeysTab 工作流所需、文件名尚无法预先确定的局部组件；不得重做其他网关 tab 或全局设计系统。
- 可见文案统一写入现有 `src/i18n.ts` 双语资源并由 `src/i18n.test.ts` 校验，不新增独立 locale 体系。

## 实施清单

- [ ] 将 KeysTab 改为与 SSH 隧道一致的展示分组 tabs；加载分组后选择当前组并请求该组列表，提供分组管理弹框完成创建、重命名和删除，明确区分展示分组与授权 `groupIds`。
- [ ] 用创建/编辑弹框替换内嵌创建卡片；创建提交名称、`displayGroupId`、过期时间、路由账号 `groupIds` 和 `modelIds`，编辑不显示或提交 key material。
- [ ] 创建成功后显示一次性密钥结果并保留显式复制能力；弹框关闭后不在组件持久状态、筛选条件、错误信息或日志中保留完整明文。
- [ ] 实现严格列顺序的 table：名称、API 密钥、分组、用量、过期时间、状态、创建时间、操作；仅展示 masked key，今日/近 30 日用量及费用不可计算状态按 DTO 原样呈现。
- [ ] 实现文本和状态筛选并与当前展示组组合查询，覆盖加载中、加载失败、无分组、空列表、无筛选结果和分页/排序状态；失败后允许重试。
- [ ] 使用 Lucide 图标实现复制、编辑、启停、重新生成、软删除和转换等操作，提供 `title`、`aria-label` 或 tooltip；busy 期间禁用重复提交且操作反馈不改变表格尺寸。
- [ ] 实现转换弹框：展示 Claude、Codex、Gemini、OpenCode 多选项，已转换工具禁选，统一激活 toggle 默认关闭；提交仅发送 `keyId`、工具数组和 `activate`。
- [ ] 每次分组、密钥或转换命令成功后按影响范围刷新分组、当前列表及可转换工具；错误时保留可恢复输入并解除 busy 以允许重试。
- [ ] 补齐中英文文案和 i18n 对称性测试，不用页面内硬编码说明文字替代交互状态与无障碍标签。
- [ ] 扩展 React 测试覆盖 tabs、三个弹框、严格列顺序、组合筛选、tooltip、禁用、busy、错误恢复、刷新调用及窄宽布局关键结构。

## 验收标准

- [ ] KeysTab 首屏直接提供可操作的展示分组 tabs 和密钥表格，默认组与自定义组行为清晰，分组删除后列表切回有效分组并刷新。
- [ ] 表格列顺序严格符合计划，密钥始终脱敏，费用 null 显示“不可计算”，状态和用量不由前端重新计算。
- [ ] 创建仅在成功结果弹框中显示一次明文；编辑不包含密钥材料字段，复制、启停、重新生成和软删除调用使用 typed wrapper。
- [ ] 文本、状态与当前分组筛选共同生效；加载失败、空态、无结果及操作错误均可恢复，不遮挡或重叠其他控件。
- [ ] 转换支持四工具多选，已转换项不可选择，统一激活默认关闭，提交载荷不含后端派生字段。
- [ ] 所有图标操作具有可访问名称/tooltip，busy 状态阻止双击重复调用；成功后相关分组、列表和工具集按需刷新。
- [ ] 中英文资源键完整且测试通过，桌面及窄宽视口内 tabs、筛选、表格和弹框内容可读且无不合理重叠。

## 验证步骤

- [ ] 运行 `npm test -- --run src/components/AiRoutingGateway/AiRoutingGateway.test.tsx src/i18n.test.ts`，确认交互、调用参数、状态、刷新和文案测试通过。
- [ ] 运行 `npm run lint` 和 `npm run build`，确认 ESLint、TypeScript typecheck 和生产构建通过。
- [ ] 启动现有 Vite/Tauri 开发环境，在桌面和窄宽视口截图检查 tabs、严格列顺序、tooltip、弹框、错误/空态和长文本布局，无重叠或内容溢出。
- [ ] 人工执行创建、一次性复制、编辑、启停、删除和多工具转换，确认 busy 防重、错误重试及命令后刷新行为。
- [ ] 检查 React state、控制台、错误提示和测试快照，确认除一次性结果外不包含完整密钥。

## 安全与回滚注意事项

- 一次性明文仅保留到显式复制或结果弹框关闭；不得写入 localStorage、查询参数、持久化 store、错误消息、console 或测试快照。
- 页面只能提交后端允许的标识和用户输入；状态、费用、可转换性、服务商 ID、base URL 与工具配置均以服务端结果为准。
- busy 锁必须覆盖每个有副作用的命令及后续刷新，避免重复转换、重复删除或过期响应覆盖最新页面状态。
- UI 回滚应恢复到 typed wrapper 仍可驱动的旧展示，不得绕过新后端约束或重新引入明文卡片列表。

## 范围外事项

- SQLite migration、Rust 领域规则、跨存储补偿和服务商 active 语义不在本任务内；页面不得复制这些规则。
