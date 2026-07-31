# 12 - 完成五页签 UI、导航与集成回归

- task_id: `12-five-tab-ui-integration-regression`
- order: `12`
- blocked_by: `11-typed-ipc-events`
- source_plan: `../plan.md`
- source_plan_digest: `385b139e1c25f8e8112982ed63ac3c3f0282be095c8322006f82f45d9070cf6d`
- write_scope: `src/App.tsx、src/lib/navigation.ts、src/components/MoreToolsHub.tsx、src/components/Launcher.tsx、src/components/AiRoutingGateway/（新建）、src/i18n.ts、public/locales/、相关前端测试与网关跨域集成测试`

## Outcome

用户可从独立导航进入“AI 路由网关”，在固定五页签中完成监控和管理；全套静态、单元、集成及构建门禁通过且现有模块行为不变。

## Implementation Checklist

- [ ] 新增与 Protocol Router 同级的独立导航目的地和模块壳，页签固定为首页、账号池、网关密钥、请求日志、设置。
- [ ] 实现首页账号状态、额度窗口、今日 Token、费用、7/15/30 日趋势及账号/分组/公开模型筛选。
- [ ] 实现账号池和账号详情，包括分组、标签、排序、OAuth 三种路径、第三方账号、阈值、额度和模型映射。
- [ ] 实现网关 Key 的一次性展示、复制、授权、重新生成、禁用、撤销和过期状态。
- [ ] 实现请求日志筛选、分页、尝试明细、不可计算费用和手动清空。
- [ ] 实现设置页端口、服务状态、全局阈值、保留期、价格及聚合维护，并覆盖加载、空、错误和锁定状态。
- [ ] 补齐中英文 i18n 文案和前端组件、导航、生命周期及隔离回归测试。

## Acceptance Criteria

- [ ] 主导航和 More Tools 均可进入独立 AI 路由网关，Protocol Router 名称、入口、路由和页面行为不变。
- [ ] 五个页签顺序固定；额度窗口和模型映射仅在账号详情编辑。
- [ ] 首页正确展示账号数量、可用进度、分母规则额度、K/M Token、不可计算费用及独立 Token/费用趋势视图，不使用双 Y 轴。
- [ ] OAuth loopback、手动完整回调和 Device Code 的全部状态及操作均有 Testing Library 覆盖。
- [ ] Key 一次性明文不会被持久保留；日志、设置和账号危险操作具备明确确认及失败状态。
- [ ] 运行中、停止、端口冲突、数据库失败、Keychain 锁定、受控重启和排空状态均有中英文文案。
- [ ] i18next 中英文键完整，导航状态可恢复，加载、空和错误状态均可判定。
- [ ] Protocol Router 和现有数据源隔离回归通过，新功能未修改 Protocol Router 内部实现。
- [ ] TypeScript、ESLint、Vitest、Rust 单元及 loopback 集成测试、`cargo test`、`cargo check` 和项目构建检查全部通过。

## Verification Steps

- [ ] 执行项目现有 TypeScript 类型检查、ESLint 和 Vitest/Testing Library 测试。
- [ ] 执行 `cargo test`、`cargo check` 及全部本机 loopback 集成测试。
- [ ] 执行项目现有构建类检查，并确认测试不依赖公网。
- [ ] 检查完整测试输出和持久化 fixture，确认无请求正文或认证凭据泄露。
- [ ] 不执行 Playwright、可见浏览器、未授权 E2E 或视觉验证。

## Out of Scope

不新增第六个顶层页签，不引入全局状态库，不改造 Protocol Router，也不执行未经授权的浏览器自动化、E2E 或视觉验证。
