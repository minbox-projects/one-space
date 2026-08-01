# 10 - 建立前端入口与模块壳

- task_id: `ai-routing-frontend-shell`
- order: `10`
- blocked_by: `ai-routing-ipc-lifecycle`
- source_plan: `../plan.md`
- source_plan_digest: `4fd0f21d03c9fd3060e030ce14e81636333e3060e47dee01c6720c5983f985ce`
- write_scope: `src/lib/aiRoutingGateway.ts；src/App.tsx；src/lib/navigation.ts；src/components/MoreToolsHub.tsx；src/components/Launcher.tsx；src/components/AiRoutingGateway/{index.tsx,types.ts,shared/**}；src/i18n.ts；public/locales/**；对应 TypeScript、Vitest 与 Testing Library 测试`

## Outcome

主导航可进入独立 AI 路由网关模块，模块具有固定五页签、typed IPC facade、共享状态视图和完整中英文文案。

## Implementation Checklist

- [ ] 建立唯一的 typed invoke facade 和事件订阅清理封装。
- [ ] 接入主导航、More Tools Hub、Launcher 和 App 目的地。
- [ ] 建立模块壳和固定五页签。
- [ ] 建立加载、空、错误、锁定、端口冲突、重启和排空共享状态。
- [ ] 添加导航、OAuth、额度、健康、Key、日志、价格、协议及安全确认文案。
- [ ] 添加 facade、导航、页签和状态组件测试。

## Acceptance Criteria

- [ ] 前端命令字符串和事件名只存在于 `src/lib/aiRoutingGateway.ts`。
- [ ] facade 为所有 IPC 输入、输出、错误和事件提供 TypeScript 类型。
- [ ] 事件订阅在组件卸载或重新订阅时可靠清理。
- [ ] 主导航目的地显示“AI 路由网关”，与 Protocol Router 同级。
- [ ] 页签顺序固定为：首页、账号池、网关密钥、请求日志、设置。
- [ ] 账号详情不是顶层页签。
- [ ] 导航状态可恢复，所有可见文案都通过 i18next。
- [ ] `public/locales/` 中英文目录与 `src/i18n.ts` 已有语言代码一一对应，键集合完全一致。
- [ ] 一次性 Key 明文不写入持久状态、localStorage、日志或错误对象。
- [ ] Protocol Router 的名称、目的地和页面行为保持不变。

## Verification Steps

- [ ] 运行 `npx tsc --noEmit`。
- [ ] 运行 `npx eslint src/lib/aiRoutingGateway.ts src/App.tsx src/lib/navigation.ts src/components/MoreToolsHub.tsx src/components/Launcher.tsx src/components/AiRoutingGateway src/i18n.ts`。
- [ ] 运行 `npx vitest run src/lib/aiRoutingGateway.test.ts src/components/AiRoutingGateway`。
- [ ] 用 Testing Library 验证独立导航、五页签顺序、状态切换和订阅清理。
- [ ] 比较中英文资源键集合，确认无缺失键。
- [ ] 检查 Protocol Router 三个既有边界无本任务差异。

## Out of Scope

不实现各页签完整业务表单、图表或请求日志列表。
