# 01 - 建立账号详情安全契约

- task_id: `gateway-account-detail-contract`
- order: `01`
- blocked_by: `none`
- source_plan: `../plan.md`
- source_plan_digest: `6f7f72cf85831fcd249370dc3a8fdcf7ecc5d3981bcc238427b969a3642dbbe1`
- write_scope: `src-tauri/src/ai_routing_gateway/{types.rs,accounts.rs,commands/mod.rs,tests.rs}、src-tauri/src/app_runtime/run_app.rs`

## 预期结果
后端提供已注册的账号专用详情命令：API Key 账号仅通过专用详情 DTO 返回解密后的明文，OAuth 账号只返回公开元数据、完整映射和价格视图，既有列表、Bootstrap、事件及错误契约继续保持无明文。

## 实施清单
- [ ] 定义账号详情、固定官方模型行、映射和价格视图 DTO，保持 `AccountDto` 不增加 API Key 字段。
- [ ] 实现详情读取与组装逻辑，并补齐历史账号缺失官方模型映射时的明确兼容行为。
- [ ] API Key 分支使用 `RootKey` 和既有 credential 解密逻辑；OAuth 分支不得读取或解密 token bundle。
- [ ] 新增 typed Tauri 详情命令并在 `run_app.rs` 注册，确保错误、事件和日志不包含 credential 或草稿内容。
- [ ] 增加 API Key、OAuth、账号不存在、RootKey/AAD/credential 异常及序列化边界测试。

## 验收标准
- [ ] API Key 详情 DTO 返回与测试 fixture 一致的完整明文及完整模型、价格详情。
- [ ] OAuth 详情的 `api_key` 为空，只含公开元数据，且测试证明未触发 OAuth credential 解密。
- [ ] `AccountDto`、账号列表、Bootstrap、账号事件和错误字符串均不包含 fixture secret。
- [ ] 详情失败仅返回稳定领域错误分类和实体标识，不回显密文、明文或 token。
- [ ] 新命令已注册，未恢复未注册的 OAuth upsert/授权命令。

## 验证步骤
- [ ] 运行详情读取、credential 解密及明文边界相关 Rust 测试并确认通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway`，确认相关模块无回归且输出不含 fixture secret。

## 范围外事项
不实现账号新增/编辑事务，不修改前端 facade 或页面，也不改变数据库 schema。
