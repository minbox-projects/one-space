# 实现账号池 Codex OAuth 登录交互

## 预期结果

依赖任务 04；在 React 账号池增加与 API Key 并列的 OAuth 添加入口，接入 typed facade 和状态事件，覆盖等待、取消、超时、错误、手动完整回调、成功后列表刷新与重新授权，并补齐本地化文案；OAuth 连接字段保持只读，现有分组、标签、备注和启用状态管理保持可用。

## 实施清单

- [ ] 将账号池添加操作改为 OAuth 与 API Key 并列的明确入口，保留现有 API Key 创建详情与验证逻辑。
- [ ] 实现 Codex OAuth 对话状态机并调用 typed facade，覆盖启动浏览器、等待自动回调、listener 失败提示、取消、超时、错误、成功和关闭清理。
- [ ] 提供手动粘贴完整 callback URL 的输入和提交操作，但不把输入值写入通知、错误详情、持久状态或日志；提交后及时清空敏感输入。
- [ ] 订阅 OAuth 状态事件并按 session ID 过滤陈旧事件；成功后关闭授权态、刷新 gateway bootstrap 和账号列表，失败后保留可操作的重试或手动完成入口。
- [ ] 对 `oauth_reauthorization_required` 账号提供重新授权操作，复用同一登录状态机并在成功后刷新原账号状态。
- [ ] OAuth 账号详情将 provider/连接字段保持只读，同时继续允许名称、分组、标签、备注和启用状态管理；API Key 连接字段继续可编辑。
- [ ] 在 `i18n.ts` 的现有语言资源中补齐 OAuth 添加、兼容风险、等待、取消、超时、手动 callback、错误、重新授权、退出登录和只读连接信息文案。

## 验收标准

- [ ] 账号池首屏可选择添加 Codex OAuth 或 API Key，两个入口都能进入完整可用流程且互不改变对方行为。
- [ ] OAuth UI 对等待、listener 失败、手动 callback、取消、超时、错误和成功都有确定终态，不会因陈旧事件更新新会话。
- [ ] 登录成功会刷新并展示账号；同一主体重新授权后刷新原账号而非显示重复账号。
- [ ] OAuth provider 与连接字段只读，分组、标签、备注和启用状态仍可保存，永久失效账号可触发重新授权。
- [ ] callback 输入及后端错误中的敏感查询参数不被 UI 回显、记录或长期保存在 React state。
- [ ] 新增界面文案在现有语言资源中均有对应键，窄屏与桌面布局无文本溢出或控件重叠。

## 验证步骤

- [ ] 运行 `npm test -- --run src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`，预期 OAuth 状态机、手动 callback、取消/超时、成功刷新、重新授权和只读字段用例通过。
- [ ] 运行 `npm test -- --run src/i18n.test.ts`，预期新增本地化键完整且资源结构测试通过。
- [ ] 运行 `npm run lint` 和 `npm run build`，预期 TypeScript、ESLint 与 Vite 构建通过。
- [ ] 启动现有前端开发环境并分别在桌面与移动视口截图检查账号池和 OAuth 对话框，预期无文本溢出、遮挡或控件重叠。

## 范围外事项

- 不允许前端直接交换 token、解析 id_token 或持久化 OAuth 凭据。
- 不重设计账号池其他标签页、网关价格或 API Key 管理流程。
