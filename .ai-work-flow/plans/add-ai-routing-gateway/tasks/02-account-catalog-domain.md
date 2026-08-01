# 02 - 实现账号池与模型目录领域

- task_id: `ai-routing-account-catalog`
- order: `02`
- blocked_by: `ai-routing-storage-security-foundation`
- source_plan: `../plan.md`
- source_plan_digest: `4fd0f21d03c9fd3060e030ce14e81636333e3060e47dee01c6720c5983f985ce`
- write_scope: `src-tauri/src/ai_routing_gateway/accounts.rs；src-tauri/src/ai_routing_gateway/models.rs；src-tauri/src/ai_routing_gateway/storage/{accounts.rs,groups.rs,tags.rs,models.rs}；src-tauri/src/ai_routing_gateway/types/{account.rs,model.rs}；对应 Rust 测试`

## Outcome

后端可按确定事务规则管理分组、标签、OAuth/第三方账号、公开模型目录和每账号模型映射。

## Implementation Checklist

- [ ] 实现默认分组、分组排序、账号排序和删除分组事务。
- [ ] 实现标签 CRUD 与账号多标签关联。
- [ ] 实现账号创建、更新、启停、备注、健康状态和永久删除。
- [ ] 实现第三方 Base URL、鉴权方式、上游协议及加密 API Key 写入。
- [ ] 实现公开模型目录和账号模型映射。
- [ ] 为永久删除建立后端二次确认令牌及事务校验。

## Acceptance Criteria

- [ ] 每个账号必须且只能属于一个分组，新账号自动进入默认分组。
- [ ] 默认分组不可删除；删除非空非默认组时，账号在同一事务中迁入默认分组。
- [ ] 标签只用于展示和筛选，不出现在路由权限或排序输入中。
- [ ] 第三方上游协议只接受 Responses 或 Chat Completions；Base URL、鉴权方式和至少一个显式模型映射通过后端校验。
- [ ] 未映射公开模型不可透传至上游。
- [ ] 凭据读取 DTO 只返回“已配置”等安全元数据，不返回 API Key 明文。
- [ ] 永久删除账号必须验证一次性确认令牌，并级联删除凭据、额度窗口和映射。
- [ ] 账号删除不删除请求日志和每日聚合中的账号快照。

## Verification Steps

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::accounts`。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::models`。
- [ ] 测试默认组保护、非空组迁移、排序稳定性、标签无路由语义和永久删除级联。
- [ ] 测试第三方 DTO、Base URL、协议枚举、映射校验及凭据输出脱敏。
- [ ] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`。

## Out of Scope

不实现 OAuth 流程、额度刷新、网关 Key、候选路由或 UI。
