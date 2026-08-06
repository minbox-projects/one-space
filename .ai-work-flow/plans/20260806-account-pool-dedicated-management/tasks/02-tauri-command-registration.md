# 02 - Tauri 原子创建命令与注册

- task_id: `tauri-command-registration`
- order: `02`
- blocked_by: `rust-domain-atomic-creation`
- source_plan: `../plan.md`
- source_plan_digest: `e8b89e919845f40f6d6d49ba0f3d16866c91d9e0cba261cc610a4b01a5976187`
- write_scope: `src-tauri/src/ai_routing_gateway/commands/mod.rs, src-tauri/src/app_runtime/run_app.rs（非穷举，仅限 command DTO、命令接线、注册及其局部测试）`

## 预期结果

Tauri 暴露并且仅注册一次 `ai_routing_gateway_account_create_api_key_with_configuration`，按固定 camelCase DTO 调用领域原子创建，并且只在提交成功后发出账号更新事件和返回现有 `AccountDto`。

## 实施清单

- [ ] 在 `commands/mod.rs` 增加 camelCase 反序列化请求 DTO，逐字段覆盖 `name`、`baseUrl`、`apiKey`、`authMethod`、`upstreamProtocol`、`note`、可选 `mappings` 和按 `publicModelId` 组织的四类可选价格。
- [ ] 将 command DTO 显式转换为 `accounts.rs` 的领域输入，不让 serde、Tauri 或前端类型泄漏到领域层。
- [ ] 新增固定命名的 Tauri command，按现有账号创建模式打开连接、获取 root key、调用组合创建并转换为 `AccountDto`。
- [ ] 确保账号更新事件只在 transaction 已成功提交后发出，所有失败路径不发成功事件。
- [ ] 在 `run_app.rs` 隔离的 AI routing gateway invoke handler 中注册新 command 一次，并更新“注册且仅一次”测试命令清单。
- [ ] 补充适合现有测试模式的 DTO/命令测试，确认字段映射和失败事件边界。

## 验收标准

- [ ] 新命令可由 invoke handler 解析，名称精确为 `ai_routing_gateway_account_create_api_key_with_configuration`，且仅注册一次。
- [ ] DTO 的 camelCase 字段与冻结 plan 的 TypeScript 契约逐字段一致，四类价格均可为空或省略。
- [ ] 成功返回不含明文密钥的现有账号 DTO；领域错误按既有命令错误转换方式返回且不发账号更新事件。
- [ ] 旧 API Key 创建、`account_update`、`mapping_save`、`price_save` 的命令名称、DTO、注册和行为未改变。

## 验证步骤

- [ ] 运行 `cargo test ai_routing_gateway_commands_are_registered_once_in_the_isolated_block --manifest-path src-tauri/Cargo.toml`，预期新旧命令均在隔离块中恰好注册一次。
- [ ] 运行 `cargo test ai_routing_gateway::commands --manifest-path src-tauri/Cargo.toml`（若模块已有可筛选测试），预期 DTO 转换和命令边界测试通过。

## 范围外事项

不修改领域事务实现、TypeScript facade 或 UI；不替换旧命令；不修改 schema、migration、错误编码或事件协议。
