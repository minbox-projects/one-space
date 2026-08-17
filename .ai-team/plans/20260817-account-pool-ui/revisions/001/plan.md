---
plan_id: 20260817-account-pool-ui
revision: "001"
target_branch: main
supersedes: null
---

# 实施计划

## 方案摘要

- 方案：在 `AccountsTab` 内复用 SSH 隧道现有的 segmented group control 与组件内下拉状态模式，合并分组管理和批量操作入口；同时调整操作行、卡片详情入口、搜索样式、i18n 和现有 Vitest 用例。
- 关键取舍：保持实现局部化，不抽取跨页面组件；“更多操作”始终显示，“添加账号”始终为右侧操作区最后一项；批量选择和后端命令语义保持原样。
- 不采用的方案及原因：不新建通用 Dropdown/GroupTabs 抽象，因为本次仅对齐一个页面且 SSH 隧道自身也是局部实现；不改后端批量接口，因为现有接口已满足集合边界、确认令牌和错误处理要求。

## 实施步骤

1. 步骤 1：在 `src/components/AiRoutingGateway/index.tsx` 的 `AccountsTab` 增加“更多操作”菜单开关状态、菜单根标记，以及点击外部和 Escape 关闭 effect；读取现有选择集合、`busy` 和分组管理状态，仅写账号池局部交互；完成条件是菜单可稳定开关，批量项禁用态和管理分组入口正确。
2. 步骤 2：在 `src/components/AiRoutingGateway/index.tsx` 重排列表头部；将分组 tabs 改为 SSH 隧道同款 segmented control，移除独立齿轮，将搜索框和右侧操作区放入稳定响应式布局；完成条件是“更多操作”和“添加账号”始终显示，后者位于最右且空状态无重复入口。
3. 步骤 3：在 `src/components/AiRoutingGateway/index.tsx` 把批量禁用、批量删除接入菜单项并在触发前关闭菜单，保留全选、已选数量、当前可见集合、确认令牌和错误状态；完成条件是成功/失败/取消语义与现有行为一致。
4. 步骤 4：在 `src/components/AiRoutingGateway/index.tsx` 删除账号卡片编辑图标按钮，保留卡片主体详情入口和移动、启停、删除、选择控件；将搜索输入类名调整为启动台基准；完成条件是两类账号详情分流和搜索行为不回归。
5. 步骤 5：在 `src/i18n.ts` 更新中英文可见文案；优先复用现有键并仅在菜单无合适键时新增账号池局部键；完成条件是“添加账号”“更多操作”及菜单项在中英文下可读且无旧入口文案残留。
6. 步骤 6：在 `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` 迁移旧入口测试并补充菜单开关、禁用态、外部点击、Escape、空分组固定操作区、无编辑图标和搜索样式断言；完成条件是 REQ-001 至 REQ-007、AC-001 至 AC-008 均有自动化证据。
7. 步骤 7：依次运行定向测试、完整前端测试、lint 和 build；失败时只修复本次变更引入的问题；完成条件是 AC-009 全部通过并记录命令结果。

## 需求覆盖

| 需求/验收 ID | 实施位置 | 验证方式 | 责任角色 |
| --- | --- | --- | --- |
| REQ-001 | `src/components/AiRoutingGateway/index.tsx` | 分组顺序、激活态、切组选择清理测试 | `frontend-developer` |
| REQ-002 | `src/components/AiRoutingGateway/index.tsx` | 菜单项目、禁用态、外部点击、Escape 测试 | `frontend-developer` |
| REQ-003 | `src/components/AiRoutingGateway/index.tsx` | 批量禁用/删除成功、失败、取消测试 | `frontend-developer` |
| REQ-004 | `src/components/AiRoutingGateway/index.tsx`、`src/i18n.ts` | 非空、空分组、搜索空状态布局测试 | `frontend-developer` |
| REQ-005 | `src/components/AiRoutingGateway/index.tsx` | API Key/OAuth 卡片主体详情测试 | `frontend-developer` |
| REQ-006 | `src/components/AiRoutingGateway/index.tsx` | 搜索类名和分组过滤测试 | `frontend-developer` |
| REQ-007 | `src/i18n.ts`、`src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` | i18n 单测与账号池定向测试 | `frontend-developer` |
| AC-001 | `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` | `npm run test -- src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` | `test` |
| AC-002 | `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` | 菜单交互测试 | `test` |
| AC-003 | `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` | 批量禁用边界测试 | `test` |
| AC-004 | `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` | 批量删除确认令牌测试 | `test` |
| AC-005 | `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` | 分组管理兼容测试 | `test` |
| AC-006 | `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` | 操作区在三种空/非空状态下的测试 | `test` |
| AC-007 | `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` | 无编辑按钮及详情分流测试 | `test` |
| AC-008 | `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` | 搜索样式和过滤测试 | `test` |
| AC-009 | `package.json` | 定向测试、`npm run test`、`npm run lint`、`npm run build` | `test` |

## 验证

- 单元测试：运行 `npm run test -- src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`，覆盖账号池新交互及原有分组、批量、详情、搜索行为。
- 集成测试：运行 `npm run test`，确认 AI 路由网关和共享 i18n 变更未影响其他前端模块。
- 静态检查：运行 `npm run lint`，重点发现 effect 依赖、未使用图标导入、可访问属性和测试代码问题。
- 构建或打包：运行 `npm run build`，确认 TypeScript、React 19 和 Vite 生产构建通过。
- 手工验证：在桌面应用账号池中检查宽屏和窄屏操作行、分组切换、下拉关闭、空分组添加、卡片详情与中英文文案；若执行角色产生截图，必须保存到 packet 提供的 `.ai-team/plans/<plan-id>/screenshot/` 精确目录。
- 失败时的诊断和回滚：定向测试失败先按菜单状态、可访问查询、选择集合或旧文案分类；lint/build 失败只修复本次引入的导入、类型和 JSX 问题；无法满足批量集合边界时停止发布并回滚实现提交。

## 发布与回滚

- 发布前门禁：AC-001 至 AC-009 全部有证据；定向测试、完整测试、lint、build 均通过；无后端、IPC 或数据迁移差异。
- 发布顺序：先合并账号池组件、i18n 和测试的单一实现提交，再随常规 OneSpace 桌面版本构建发布。
- 监控和观察窗口：发布后首轮人工检查账号池分组切换、菜单操作、空分组添加和两类详情入口；关注批量命令错误反馈和确认令牌调用。
- 回滚条件：菜单无法访问管理分组或批量操作、批量账号集合超出当前可见选择、添加入口在空分组消失、卡片无法进入正确详情、完整前端质量门禁失败。
- 回滚命令：由 Git Operator 对实现提交执行 `git revert <implementation-commit>`；无需数据库或后端回滚。
