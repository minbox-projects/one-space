# 12 - 完成观测页面与工程收口

- task_id: `ai-routing-observability-ui-regression`
- order: `12`
- blocked_by: `ai-routing-operations-ui`
- source_plan: `../plan.md`
- source_plan_digest: `4fd0f21d03c9fd3060e030ce14e81636333e3060e47dee01c6720c5983f985ce`
- write_scope: `src/components/AiRoutingGateway/{HomePage/**,RequestLogsPage/**}；src/components/AiRoutingGateway/__tests__/**；AI 路由网关相关前端测试 fixtures`

## Outcome

首页和请求日志页完整呈现额度、Token、费用、趋势及尝试明细，且整个项目通过 Rust、TypeScript、ESLint、Vitest 和构建门禁。

## Implementation Checklist

- [ ] 实现首页账号、可用性、额度、今日 Token 和费用摘要。
- [ ] 实现 7/15/30 日 Token 与费用趋势及筛选。
- [ ] 实现请求日志筛选、游标分页和尝试明细。
- [ ] 实现不可计算费用、空数据和缺日补零展示。
- [ ] 补齐五页签集成测试和现有模块回归测试。
- [ ] 运行并修复完整静态、单元、集成和构建检查。

## Acceptance Criteria

- [ ] 首页展示账号总数、可用与不可用数量及进度。
- [ ] 5 小时、7 日和附加窗口按后端聚合结果独立展示，缺窗口账号不被错误计入。
- [ ] 今日 Token 展示总量、输入和输出，并按统一规则格式化为原值、K 或 M。
- [ ] 趋势周期只提供 7/15/30 日分段控制。
- [ ] Token 趋势提供输入、输出、缓存和总量视图。
- [ ] 费用使用独立视图，不使用双 Y 轴，并标明 OAuth 为公开 API 单价等效预估。
- [ ] 趋势支持全部账号、单账号、分组和公开模型筛选。
- [ ] 价格或用量缺失显示不可计算，不显示为 `$0`。
- [ ] 请求日志支持计划规定的全部筛选、稳定分页和账号尝试展开。
- [ ] 尝试明细展示顺序、账号快照、时间、上游状态、错误类别、流字节状态及健康影响。
- [ ] 手动清空有明确确认和后台进度，失败不会让已存在数据在 UI 中被误报为清空。
- [ ] 加载、空、错误和锁定状态具有中英文文案。
- [ ] Protocol Router 的入口、配置、统计、运行行为和代码边界无回归。
- [ ] 不引入 Playwright、浏览器自动化、E2E 或视觉验证作为本次验收。

## Verification Steps

- [ ] 运行 `npx vitest run src/components/AiRoutingGateway`，所有 facade 与组件测试通过。
- [ ] 运行 `npx tsc --noEmit`。
- [ ] 运行 `npx eslint .`。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml`，包括 Rust 单元和 loopback 集成测试。
- [ ] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`。
- [ ] 运行 `npm run build`，项目构建通过。
- [ ] 执行敏感标记测试，确认数据库、tracing、IPC、HTTP 错误及前端日志均无正文和凭据。
- [ ] 检查 Protocol Router 三个既有边界及其回归测试，确认未接入新网关状态、监听器、类型或命令空间。

## Out of Scope

不执行浏览器自动化、Playwright、E2E、视觉验证、远程网络测试或真实公网 OAuth/上游调用。
