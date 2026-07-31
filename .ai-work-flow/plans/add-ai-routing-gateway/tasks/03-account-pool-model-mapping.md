# 03 - 实现账号池、分组标签与模型映射

- task_id: `03-account-pool-model-mapping`
- order: `03`
- blocked_by: `01-shared-sqlite-schema, 02-keychain-credential-security`
- source_plan: `../plan.md`
- source_plan_digest: `037804aa9bfa9cdfc9001966bb673f99116f870c328e29c2f1e5ad7aa4c79d19`
- write_scope: `src-tauri/src/ai_routing_gateway/{accounts.rs,groups.rs,models.rs,tests/accounts.rs}`

## Outcome

后端能够事务化管理 OAuth 与第三方账号、单分组、多标签、排序、状态和公开模型映射，并严格执行默认分组及永久删除规则。

## Implementation Checklist

- [ ] 实现分组、标签、账号、模型目录和账号模型映射的领域服务及事务校验。
- [ ] 实现默认分组不可删除、删除非空组时原子迁移账号、分组和账号显式排序。
- [ ] 实现第三方账号 Base URL、鉴权方式、Responses/Chat 上游协议和加密 API Key 的录入与更新。
- [ ] OAuth 账号根据官方模型目录生成可逐项禁用的默认映射；第三方账号显式维护公开模型到上游模型映射。
- [ ] 永久删除账号要求后端校验二次确认令牌，并删除凭据、额度和映射但保留历史快照。
- [ ] 保证标签仅用于识别和筛选，不进入路由权限或候选排序。

## Acceptance Criteria

- [ ] 新账号自动进入默认分组，账号始终且仅属于一个分组，可关联多个标签。
- [ ] 默认分组删除失败；删除非空普通分组后账号在同一事务内迁入默认分组。
- [ ] 无效 Base URL、鉴权方式、协议类型或模型映射被稳定拒绝，失败事务不留下部分数据。
- [ ] 未映射公开模型不能被解析为上游模型，也不会默认透传同名模型。
- [ ] 所有账号读取 DTO 只返回“凭据已配置”等安全元数据，不返回第三方 API Key 或 OAuth Token。
- [ ] 永久删除后凭据、额度和模型映射消失，已有请求日志及每日聚合的账号快照仍可读取。

## Verification Steps

- [ ] 执行账号领域 Rust 测试，覆盖 CRUD、排序、标签、默认组迁移和事务失败回滚。
- [ ] 执行第三方账号与模型映射测试，覆盖成功、无效输入和未映射模型。
- [ ] 执行账号永久删除及历史快照保留测试。

## Out of Scope

不实现 OAuth 授权状态机、额度刷新、网关 Key 或前端账号池页面。
