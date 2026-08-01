# 11 - 实现运维管理页面

- task_id: `ai-routing-operations-ui`
- order: `11`
- blocked_by: `ai-routing-frontend-shell`
- source_plan: `../plan.md`
- source_plan_digest: `4fd0f21d03c9fd3060e030ce14e81636333e3060e47dee01c6720c5983f985ce`
- write_scope: `src/components/AiRoutingGateway/{AccountsPage/**,AccountDetail/**,OAuth/**,ApiKeysPage/**,SettingsPage/**}；对应 Vitest 与 Testing Library 测试`

## Outcome

用户可在账号池、账号详情、网关密钥和设置页面完成全部日常管理操作，并得到明确安全确认和服务状态反馈。

## Implementation Checklist

- [ ] 实现分组、标签、账号排序、筛选、启停、备注和健康展示。
- [ ] 实现 OAuth loopback、手动完整回调和 Device Code 三种交互。
- [ ] 实现第三方账号录入和模型映射编辑。
- [ ] 实现账号详情额度、过期状态和阈值继承/覆盖。
- [ ] 实现永久删除二次确认。
- [ ] 实现 Key 创建、一次性明文、复制、授权、重新生成、禁用和撤销。
- [ ] 实现端口、服务状态、全局阈值、保留期、价格和聚合维护设置。

## Acceptance Criteria

- [ ] 账号池支持分组和账号排序、标签筛选、备注、启停和健康状态。
- [ ] 账号详情是额度窗口、账号阈值和模型映射的唯一编辑入口。
- [ ] OAuth UI 展示 loopback 状态、手动完整回调输入、Device Code、验证地址和倒计时。
- [ ] Device Code 提供复制、打开和取消动作，并展示 pending、slow_down、过期、取消和成功状态。
- [ ] 第三方表单必须提交 Base URL、鉴权方式、上游协议及显式模型映射。
- [ ] 永久删除必须二次确认并使用后端确认令牌。
- [ ] Key 明文只在创建或重新生成成功界面显示一次；关闭后无法再次读取。
- [ ] Key 页支持多分组、多公开模型授权、禁用、撤销和过期展示。
- [ ] 设置页端口只允许有效端口值，绑定地址固定展示为 `127.0.0.1` 且不可编辑。
- [ ] 保留期只提供 7/30/90/180 天和永久。
- [ ] 数据库失败、Keychain 锁定、端口冲突、重启和排空状态禁用不安全操作并显示对应恢复信息。

## Verification Steps

- [ ] 运行 `npx tsc --noEmit`。
- [ ] 运行 `npx eslint src/components/AiRoutingGateway`。
- [ ] 运行 `npx vitest run src/components/AiRoutingGateway/AccountsPage src/components/AiRoutingGateway/AccountDetail src/components/AiRoutingGateway/OAuth src/components/AiRoutingGateway/ApiKeysPage src/components/AiRoutingGateway/SettingsPage`。
- [ ] 用 Tauri mock 覆盖分组迁移、排序、删除确认、三种 OAuth 路径和第三方录入。
- [ ] 验证 Key 一次性明文、重新生成、权限、禁用、撤销和过期。
- [ ] 验证设置校验、端口状态、价格维护和聚合维护进度。

## Out of Scope

不实现首页趋势和请求日志浏览页面，不执行浏览器自动化或视觉验证。
