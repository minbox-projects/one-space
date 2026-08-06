# 01 - Rust 领域原子创建与 OAuth 写防线

- task_id: `rust-domain-atomic-creation`
- order: `01`
- blocked_by: `none`
- source_plan: `../plan.md`
- source_plan_digest: `e8b89e919845f40f6d6d49ba0f3d16866c91d9e0cba261cc610a4b01a5976187`
- write_scope: `src-tauri/src/ai_routing_gateway/accounts.rs, src-tauri/src/ai_routing_gateway/pricing.rs（非穷举，仅限领域实现及其同文件测试）`

## 预期结果

Rust 领域层能够在一个 SQLite transaction 内原子创建 API Key 账号、密文凭据、默认组、完整最终映射和非空价格快照，并在任一失败时零残留，同时拒绝 OAuth 或不存在账号的映射写入。

## 实施清单

- [ ] 在 `accounts.rs` 定义不依赖 command DTO 的组合创建输入、显式映射输入和按公开模型组织的四类可选价格输入。
- [ ] 收敛现有 API Key 创建的连接校验、密钥加密、默认组解析、账号与凭据插入及默认映射构造为 transaction 可复用 helper，保持旧创建函数行为不变。
- [ ] 在开启事务前完成连接字段和所有非空价格格式校验；拒绝空显式价格、负数、科学计数法和超过既有精度的值。
- [ ] 在事务内读取官方模型，生成同名且启用的完整默认映射，以显式映射按 `public_model_id` 覆盖，并拒绝未知公开模型 ID。
- [ ] 在 `pricing.rs` 提取 transaction-safe 的账号类型检查、十进制解析和 `account_override` 快照插入入口，同时保留 `save_price(&Connection, ...)` 的签名与历史语义。
- [ ] 仅为四类价格至少一项非空的模型写入快照，并沿用统一创建时刻和现有历史快照列。
- [ ] 提交前完成账号、密文凭据、默认组、映射和价格写入；依靠 transaction drop 回滚校验或 SQL 失败，不在领域层发布事件。
- [ ] 为 `set_model_mapping` 增加账号存在性和 `api_key` 类型检查，使 OAuth 返回既有 invalid 类错误、不存在账号返回既有 not-found 类错误。
- [ ] 在所属 Rust 模块测试中覆盖成功数据、密钥不落明文、默认映射、显式覆盖、空价格跳过、非法输入与强制后续失败零残留、OAuth 映射拒绝，以及旧价格入口不回归。

## 验收标准

- [ ] 成功创建后，每个官方模型恰有一条最终映射，未显式覆盖的映射同名且启用，密钥只通过既有加密机制持久化。
- [ ] 账号、凭据、默认组、映射或价格任一步失败时，本请求在所有相关表中均无残留。
- [ ] 空价格不产生价格快照；任一非空价格非法时整个组合创建失败且不提交。
- [ ] 旧 API Key 创建及 `pricing::save_price` 的公开签名、校验、历史快照和 OAuth 拒绝语义保持不变。
- [ ] 直接对 OAuth 或不存在账号调用映射保存时，不会创建或修改映射，并返回现有可识别领域错误。

## 验证步骤

- [ ] 运行 `cargo test ai_routing_gateway::accounts --manifest-path src-tauri/Cargo.toml`，预期领域原子性、映射和 OAuth 防线测试全部通过。
- [ ] 运行 `cargo test ai_routing_gateway::pricing --manifest-path src-tauri/Cargo.toml`，预期 transaction helper 与旧价格入口测试全部通过。

## 范围外事项

不修改 Tauri command、运行时注册、TypeScript facade 或前端组件；不发布 Tauri 事件；不修改任何 schema、migration 或既有错误编码。
