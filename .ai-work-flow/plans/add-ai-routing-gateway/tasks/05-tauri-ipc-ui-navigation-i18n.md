# 05 - Tauri IPC、Typed Facade、导航、五页签 UI 与 i18n

- task_id: `task-05`
- order: `05`
- blocked_by: `task-04`
- source_plan: `../plan.md`
- source_plan_digest: `92f85a7f07acc328e48edf775eae5bfb751f58861b7c1b93de18fb68ed5fd822`
- write_scope: `src-tauri/src/ai_routing_gateway/commands/；src/lib/aiRoutingGateway.ts；src/components/AiRoutingGateway/；src/App.tsx；src/lib/navigation.ts；src/components/MoreToolsHub.tsx；src/components/Launcher.tsx；src/i18n.ts；相关前端测试`

## Outcome

用户可通过独立导航入口和五页签界面管理及观察 AI 路由网关，所有前端交互只经过类型化 facade，并具备完整中英文资源和前端自动化测试。

## Implementation Checklist

- [x] 唯一负责 `src-tauri/src/ai_routing_gateway/commands/` 中全部 IPC command 实现及事件载荷转换，但不修改 `run_app.rs` 的注册列表。
- [x] 唯一负责 `src/lib/aiRoutingGateway.ts`；所有 invoke 字符串、DTO 映射、事件订阅和释放集中于此。
- [x] 唯一负责 `src/components/AiRoutingGateway/` 五页签模块：首页、账号池、网关密钥、请求日志、设置。
- [x] 负责接入 `src/App.tsx`、`src/lib/navigation.ts`、`src/components/MoreToolsHub.tsx`、`src/components/Launcher.tsx` 的既有导航模型。
- [x] 唯一负责扩充 `src/i18n.ts` 内联中英文资源；不得创建 `public/locales/`。

## Acceptance Criteria

- [x] IPC 统一使用 `ai_routing_gateway_*` 前缀，覆盖计划要求的 runtime/settings、领域管理、OAuth、额度、Key、日志、价格、统计和维护能力。
- [x] 写命令使用明确 DTO、后端校验和事务；所有读取结果均不包含 OAuth token、第三方 API Key 或历史网关 Key 明文。
- [x] 组件中不存在散布的 Tauri command 字符串或对 Rust 存储结构的直接依赖，事件订阅均可释放。
- [x] 首页支持账号、分组、公开模型筛选，以及容量、额度窗口、Token 分项、费用和 7/15/30 日独立趋势视图。
- [x] 账号池覆盖排序、标签、启停、健康、备注、OAuth 三路径、第三方账号、额度阈值、映射和永久删除确认。
- [x] Key 页覆盖一次性明文、复制、权限、重生成、禁用、撤销和过期，明文不进入持久前端状态或日志。
- [x] 日志页覆盖筛选、稳定分页、attempt 明细、不可计算费用和清空；设置页覆盖端口、服务、阈值、保留、价格和聚合维护。
- [x] 主导航、More Tools Hub 和 Launcher 均可进入模块，五页签状态与错误、锁定、端口冲突、维护进度可正确展示。
- [x] `src/i18n.ts` 中英文键集合一致且覆盖全部新增界面。
- [x] Vitest、Testing Library 和 Tauri mock 覆盖 facade、event cleanup、导航、五页签及关键状态，不运行浏览器或 E2E。

## Verification Steps

- [x] 执行本任务 Acceptance Criteria 对应的 Vitest、Testing Library 和 Tauri mock 测试并确认全部通过。

## Verification Evidence

- IPC 与领域：`cargo test --lib ai_routing_gateway`，67 项通过；`cargo test --lib` 全量 435 项通过、2 项既有本地 smoke test 忽略；`commands::tests::all_public_commands_use_the_isolated_prefix` 验证独立前缀，既有安全/账号/Key/日志/统计测试验证脱敏、事务及一次性明文契约。
- Facade 与事件：`src/lib/aiRoutingGateway.test.ts` 验证 command/DTO 映射、错误归一化和四类 listener cleanup。
- 五页签与状态：`src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` 验证五页签、空状态、OAuth 发布门禁、锁定、端口冲突和一次性 Key 明文关闭清理。
- 导航与 i18n：`src/lib/navigation.test.ts`、`src/components/MoreToolsHub.test.tsx`、`src/components/Launcher.test.tsx`、`src/App.moreToolsNavigation.test.tsx`、`src/i18n.test.ts` 验证稳定入口、Launcher/More Tools 分发及中英文新增键集合一致。
- 非浏览器门禁：`npm run build`、`npm run lint`、`npm run test`（32 个测试文件、272 项）均通过；未运行浏览器、Playwright、E2E 或截图。

## Out of Scope

不修改 `run_app.rs`、Protocol Router 或后端生命周期；不创建 `public/locales/`；不运行浏览器或 E2E。
