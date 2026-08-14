---
plan_id: 20260814-account-pool-routing-a1b2
revision: "001"
target_branch: main
supersedes: null
---

# 实施计划

## 方案摘要

- 方案：在 `AccountsTab` 的两个添加入口前增加账号类型 Dialog，将创建视图状态显式携带 `oauth` 或 `api_key`；按类型渲染 API Key 新增页或 OAuth 暂不可用页。编辑时继续从已加载账号的持久化 `account_type` 选择类型化视图，并保持后端接口不变。
- 关键取舍：复用现有组件内部详情状态和 Radix Dialog；只改前端视图、i18n、测试及受管导航上下文；不新增 OAuth IPC、数据库迁移或账号类型更新能力。
- 不采用的方案及原因：不实现完整 OAuth enrollment，因为官方第三方契约和注册 IPC 不可用；不新增应用级路由，因为当前账号详情由组件内部状态管理；不抽取新跨模块表单框架，因为改动集中且现有模式足够。

## 实施步骤

1. 在 `src/components/AiRoutingGateway/index.tsx` 调整账号池创建/详情状态：两个添加入口统一打开类型选择 Dialog；取消恢复账号池；确认后携带不可变类型进入对应新增视图。完成条件：REQ-001、REQ-003、AC-001、AC-003 的交互可由组件测试观察，选择和关闭均不调用写 IPC。
2. 在同一组件中拆清类型化新增与编辑渲染：API Key 新增/编辑复用现有表单和保存函数；OAuth 新增渲染暂不可用状态；现有账号编辑严格读取 `account.account_type`，不提供类型编辑控件。完成条件：REQ-002、REQ-004、REQ-005、REQ-006、REQ-007 与 AC-002、AC-004、AC-005、AC-006、AC-007 均满足，typed IPC payload 不变。
3. 在 `src/i18n.ts` 增加中英文类型选择、OAuth 暂不可用、类型化标题与操作文案，并复用现有公共取消/返回文案。完成条件：两种语言下无缺失 key，紧凑弹框与详情标题不溢出。覆盖 REQ-008、AC-008。
4. 在 `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` 扩展回归测试：覆盖空/非空添加入口、Dialog 取消与 Escape、两种创建分流、OAuth 不调用写 IPC、两种卡片及编辑按钮分流、类型不可编辑、API Key 既有行为和 OAuth 通用元数据保存。仅在 facade 行为断言确有缺口时更新 `src/lib/aiRoutingGateway.test.ts`，不修改 facade 契约。完成条件：REQ-001 至 REQ-008、AC-001 至 AC-008 均有自动化证据。
5. 实现角色通过 `ai-team context update` 同步 `MEMORY.md` 与 `.ai-work-flow/index/feature-navigation.md` 中账号池入口行为和 OAuth release gate 描述，再运行上下文校验、聚焦测试、lint 与 build。完成条件：受管文档与实现一致，所有门禁通过。

## 需求覆盖

| 需求/验收 ID | 实施位置 | 验证方式 | 责任角色 |
| --- | --- | --- | --- |
| REQ-001 | `src/components/AiRoutingGateway/index.tsx` | 类型选择与取消组件测试 | frontend-developer |
| REQ-002 | `src/components/AiRoutingGateway/index.tsx` | API Key 新增回归与 IPC 参数断言 | frontend-developer |
| REQ-003 | `src/components/AiRoutingGateway/index.tsx` | OAuth 暂不可用且无写调用测试 | frontend-developer |
| REQ-004 | `src/components/AiRoutingGateway/index.tsx` | 无类型控件、payload 无 `account_type` | frontend-developer |
| REQ-005 | `src/components/AiRoutingGateway/index.tsx` | 两种账号与两种点击入口分流测试 | frontend-developer |
| REQ-006 | `src/components/AiRoutingGateway/index.tsx` | 现有 API Key 编辑测试 | frontend-developer |
| REQ-007 | `src/components/AiRoutingGateway/index.tsx` | OAuth 通用元数据保存与专属字段只读测试 | frontend-developer |
| REQ-008 | `src/i18n.ts`、`src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` | 双语检查、键盘交互、lint/build | frontend-developer |
| AC-001 | `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` | `npm run test -- src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` | test |
| AC-002 | `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` | 有效、无效、取消与失败用例 | test |
| AC-003 | `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` | OAuth 页面与零写 IPC 断言 | test |
| AC-004 | `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` | 类型不可编辑与更新参数断言 | test |
| AC-005 | `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` | 卡片主体和编辑按钮分流用例 | test |
| AC-006 | `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`、`src/lib/aiRoutingGateway.test.ts` | API Key 编辑回归 | test |
| AC-007 | `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` | OAuth 允许/禁止字段断言 | test |
| AC-008 | `src/i18n.ts`、`MEMORY.md`、`.ai-work-flow/index/feature-navigation.md` | test、lint、build、context validate | frontend-developer |

## 验证

- 单元测试：`npm run test -- src/components/AiRoutingGateway/AiRoutingGateway.test.tsx src/lib/aiRoutingGateway.test.ts`。
- 集成测试：组件层 mock typed IPC，验证从账号池入口到类型化新增/编辑保存的完整状态流；本次无新后端接口，因此不新增 Rust 集成测试。
- 静态检查：`npm run lint`；`ai-team context validate --project /Users/yuqiyu/AiHistorys/one-space/onespace-app`。
- 构建或打包：`npm run build`。
- 手工验证：在中英文下检查空账号池和非空工具栏两个添加入口；检查 Dialog 鼠标/键盘取消；检查 OAuth 暂不可用返回；分别点击 OAuth/API Key 卡片主体和编辑按钮；确认类型不可编辑且页面无文本溢出或控件重叠。
- 失败时的诊断和回滚：测试失败先区分视图状态、i18n key、mock 调用次数和既有 IPC 参数差异；若 API Key 回归或 OAuth 发生写调用，回滚本次账号池组件改动并保留测试证据，不触碰数据库。

## 发布与回滚

- 发布前门禁：AC-001 至 AC-008 全部有证据；聚焦测试、lint、build、context validate 全部通过；评审确认无 OAuth enrollment 路径和无 `account_type` 更新字段。
- 发布顺序：合入组件、测试、i18n 与受管上下文的同一实现提交，随后按现有 OneSpace 桌面端发布流程构建。
- 监控和观察窗口：发布后首轮人工检查添加入口、两种类型分流、API Key 保存错误率和现有账号编辑；不新增遥测字段。
- 回滚条件：添加入口不可用、API Key 创建或编辑回归、OAuth 触发写入、类型可被修改、双语关键文案缺失。
- 回滚命令：由 Git Operator 对实现提交执行非破坏性 revert；本次无数据迁移，无需数据库回滚。
