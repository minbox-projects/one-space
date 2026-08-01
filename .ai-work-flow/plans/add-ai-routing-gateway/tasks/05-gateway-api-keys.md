# 05 - 实现网关 API Key

- task_id: `ai-routing-gateway-keys`
- order: `05`
- blocked_by: `ai-routing-account-catalog`
- source_plan: `../plan.md`
- source_plan_digest: `4fd0f21d03c9fd3060e030ce14e81636333e3060e47dee01c6720c5983f985ce`
- write_scope: `src-tauri/src/ai_routing_gateway/api_keys.rs；src-tauri/src/ai_routing_gateway/storage/api_keys.rs；src-tauri/src/ai_routing_gateway/types/api_key.rs；对应 Rust 测试`

## Outcome

应用可签发、验证、授权、重新生成、禁用和撤销不可反查的网关 API Key，明文只在成功创建或重新生成时返回一次。

## Implementation Checklist

- [ ] 使用密码学安全随机源生成高熵 Key 和可见前缀。
- [ ] 保存加盐哈希或等效安全验证材料。
- [ ] 实现分组和公开模型多选授权事务。
- [ ] 实现重新生成、禁用、撤销、过期和最后使用时间更新。
- [ ] 定义只出现一次明文的创建与重新生成 DTO。
- [ ] 实现恒定时间或密码哈希库提供的安全验证。

## Acceptance Criteria

- [ ] 数据库不保存网关 Key 明文或可逆密文。
- [ ] 普通查询 DTO 永不返回明文；创建和重新生成成功 DTO 各只返回一次。
- [ ] 重新生成在一个事务中撤销旧材料并启用新材料。
- [ ] 禁用、撤销和过期立即影响后续验证，不依赖服务重启。
- [ ] Key 可授权一个或多个分组及公开模型，但不能绑定标签。
- [ ] 无效、禁用、撤销、过期和权限不足被区分为稳定认证结果。
- [ ] 日志可引用 Key 内部 ID、名称快照和前缀，但不包含明文或验证材料。
- [ ] 成功验证后的最后使用时间更新不阻塞请求主路径。

## Verification Steps

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::api_keys`。
- [ ] 验证 Key 熵、不同盐、正确与错误 Key、重新生成、禁用、撤销和过期。
- [ ] 验证分组/模型授权交集及标签不参与权限。
- [ ] 扫描数据库和测试日志，确认不存在 Key 明文。
- [ ] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`。

## Out of Scope

不实现 HTTP Bearer 中间件、账号候选选择、网关 Key UI 或请求日志页面。
