# 01 - 实现完整账号创建、分组与批量命令

- task_id: `implement-account-pool-backend`
- order: `01`
- blocked_by: `none`
- source_plan: `../plan.md`
- source_plan_digest: `5580666a0b5285182d47ad850a271e4f8faf8cec0b380701a079849ff084ea1d`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src-tauri/src/ai_routing_gateway/accounts.rs`
  - `src-tauri/src/ai_routing_gateway/commands/mod.rs`
  - `src-tauri/src/app_runtime/run_app.rs`
  - `src-tauri/src/shared_sqlite/migrations.rs`
  - `src-tauri/src/ai_routing_gateway/schema_v5.sql`

## 预期结果

扩展 Rust/Tauri 创建契约，在单次事务中校验并持久化连接与认证、分组、标签、额度阈值、备注、模型映射和价格配置；实现并注册分组重命名、删除迁移、批量禁用和批量删除命令，必要时追加兼容迁移。验收以完整字段可读回、既有账号兼容、非法输入整体回滚、默认组受保护、删除组时账号原子迁移、批量目标全量校验、禁用幂等及删除确认语义正确为准。

## 实施清单

- [ ] 扩展 `CreateApiKeyAccount`、`CreateApiKeyAccountWithConfiguration` 及对应 Tauri camelCase 输入 DTO，使一次请求包含名称、API 地址、API Key、认证方式、上游协议、`groupId`、标签、额度阈值、备注、模型映射和价格配置；保留既有简单创建及账号读取兼容性。
- [ ] 在开启写事务前完成可独立完成的字段规范化，在同一事务内校验目标分组存在、额度阈值位于 0 至 100、标签可持久化、模型映射引用官方模型且上游模型非空、价格字段合法；账号、加密凭据、标签关联、映射和账号价格必须一起提交或一起回滚。
- [ ] 复用现有 `GatewayError` 分类和账号事件发射方式，确保无效字段、目标不存在、名称冲突、确认无效及存储失败可由前端区分，且错误中不暴露 API Key 或加密载荷。
- [ ] 在数据访问层增加自定义分组重命名：验证分组存在、名称非空且不冲突，拒绝默认组；保持现有删除分组事务，在删除前定位默认组并原子迁移全部账号，任何失败均不得留下部分迁移或已删分组。
- [ ] 定义并实现接收显式账号 ID 集合的批量禁用命令；拒绝空集合和重复/无效目标策略不一致的输入，写入前校验全部账号存在且可操作，在单事务内禁用全部目标，并将已经禁用的账号视为幂等成功。
- [ ] 将现有单账号删除确认语义扩展为可绑定完整账号 ID 集合的批量确认；确认令牌必须有时效、只能消费一次且不能用于不同集合。批量删除在单事务中先校验全部目标和确认，再删除全部目标，失败不得部分删除。
- [ ] 在 `run_app.rs` 注册分组重命名、批量禁用、批量删除确认和批量删除命令，命令名统一使用 `ai_routing_gateway_` 前缀；命令成功后按既有模式发出足以触发前端重新读取的账号事件或返回明确结果。
- [ ] 核对现有 v1 schema 已包含分组、标签、额度阈值、映射和价格字段。只有发现现有 schema 无法表达必要约束时才新增 `schema_v5.sql` 并登记到 `shared_sqlite/migrations.rs`；迁移必须向前兼容已有账号和默认组，不重写业务数据。若无需迁移，不创建或修改这两个迁移文件。

## 验收标准

- [ ] 完整创建返回的账号可读回指定分组、去除无效空白后的标签、额度阈值、备注、连接和认证信息、协议、模型映射及价格覆盖；凭据仍只以现有加密形式持久化。
- [ ] 非法分组、越界阈值、未知公开模型、空上游模型或非法价格中的任一项都会令创建整体失败，数据库中不存在账号、凭据、标签关联、映射或价格残留。
- [ ] 既有账号及默认分组无需数据修复即可继续读取；旧的单账号更新、移动、删除确认与删除命令行为不回归。
- [ ] 默认组不可重命名或删除；删除自定义组时，该组账号在同一事务内迁移到默认组，迁移或删除失败时原状态不变。
- [ ] 批量命令仅作用于请求中的显式 ID 集合；任一 ID 不存在或不可操作时不产生部分写入；批量禁用包含已禁用账号时仍成功。
- [ ] 批量删除必须使用与目标集合严格绑定的有效确认，取消、过期、复用或集合不匹配均不删除账号；成功后全部目标不可再读取。
- [ ] 新命令已注册，Rust DTO 的 `serde(rename_all = "camelCase")` 映射与预定前端参数一致，错误结果可被统一 IPC facade 处理。

## 验证步骤

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::accounts`，预期完整创建、回滚、分组保护/迁移、批量禁用和批量删除相关测试全部通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::commands`，预期新命令 DTO、错误映射、事件及确认语义测试通过。
- [ ] 若新增迁移，运行 `cargo test --manifest-path src-tauri/Cargo.toml shared_sqlite`，预期空库和已有 v4 数据库均可升级，默认组与已有账号保持可读；若未新增迁移，确认 diff 中不存在 schema 或迁移登记改动。
- [ ] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`，预期新增命令注册和所有 Rust 类型检查通过。

## 范围外事项

- 前端 TypeScript facade、账号池 UI、分组 tabs、选择状态与文案不在本任务实施。
- 不引入新的认证类型、服务商专属字段、无选择的整组批量操作或无关数据库重构。
- 不修改已有 migration SQL；除非现有 schema 确实不足，否则不新增迁移。
