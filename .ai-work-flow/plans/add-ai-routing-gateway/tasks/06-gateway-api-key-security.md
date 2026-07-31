# 06 - 实现网关 API Key 与授权策略

- task_id: `06-gateway-api-key-security`
- order: `06`
- blocked_by: `02-keychain-credential-security, 03-account-pool-model-mapping`
- source_plan: `../plan.md`
- source_plan_digest: `385b139e1c25f8e8112982ed63ac3c3f0282be095c8322006f82f45d9070cf6d`
- write_scope: `src-tauri/src/ai_routing_gateway/{api_keys.rs,authentication.rs,tests/api_keys.rs}`

## Outcome

用户可安全创建、重新生成、禁用、撤销和设置过期网关 Key，并以分组和公开模型授权限制每个 Key 的访问范围。

## Implementation Checklist

- [ ] 使用密码学安全随机源生成高熵网关 Key，并仅在创建或重新生成成功结果中返回一次明文。
- [ ] 数据库仅保存内部 ID、可见前缀、加盐哈希或等效验证材料以及非敏感元数据。
- [ ] 实现恒定时间或安全库验证、启用、撤销、过期及分组和公开模型授权检查。
- [ ] 实现重新生成时原子撤销旧验证材料，禁用、撤销和过期立即影响新请求。
- [ ] 认证成功后异步更新最后使用时间，日志上下文只保留 Key 内部 ID 和名称快照。

## Acceptance Criteria

- [ ] 创建结果仅一次包含明文，后续查询只能返回前缀和非敏感状态。
- [ ] 正确 Key 可验证，错误、禁用、撤销和过期 Key 返回可区分的稳定认证类别。
- [ ] 重新生成成功后旧 Key 立即失效，新 Key 可用；事务失败时旧 Key 仍保持有效。
- [ ] Key 可授权一个或多个分组和公开模型，标签不作为授权维度。
- [ ] 测试扫描数据库、错误、tracing 和 DTO，确认不存在网关 Key 明文。
- [ ] Key 熵、加盐验证材料和比较方式通过安全单元测试。

## Verification Steps

- [ ] 执行网关 Key 创建、验证、重新生成、禁用、撤销、过期和授权关系测试。
- [ ] 执行一次性明文与敏感数据扫描测试。
- [ ] 执行 `cargo test` 与 `cargo check`。

## Out of Scope

不启动 HTTP 服务，不实现候选账号路由，也不把 Key 权限扩展到标签或远程用户。
