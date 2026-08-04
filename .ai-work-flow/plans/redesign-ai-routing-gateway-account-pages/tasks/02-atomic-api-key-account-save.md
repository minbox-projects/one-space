# 02 - 实现 API Key 账号原子保存

- task_id: `atomic-api-key-account-save`
- order: `02`
- blocked_by: `gateway-account-detail-contract`
- source_plan: `../plan.md`
- source_plan_digest: `6f7f72cf85831fcd249370dc3a8fdcf7ecc5d3981bcc238427b969a3642dbbe1`
- write_scope: `src-tauri/src/ai_routing_gateway/{types.rs,accounts.rs,pricing.rs,commands/mod.rs,tests.rs}`

## 预期结果
后端通过单个 typed 命令在一个 SQLite transaction 中原子新增或编辑 API Key 账号、加密凭据、标签、固定官方模型映射和字段级价格覆盖，失败时不产生任何部分写入。

## 实施清单
- [ ] 定义共享保存草稿及映射、四类可空价格覆盖输入，以可选 `account_id` 区分新增和编辑。
- [ ] 提取可接收 `Transaction` 的账号、credential、标签、映射和价格保存 helper，保留旧兼容命令所需行为。
- [ ] 在开启事务前校验账号类型、连接字段、固定官方模型集合、映射名称、阈值和十进制价格。
- [ ] 新增时创建完整官方模型同名启用映射且不复制官方价格；编辑时补齐新增官方模型并禁止提交非官方模型或删除模型行。
- [ ] 将 `null` 价格覆盖实现为删除对应账号字段覆盖并继承官方值；禁用映射时保留名称和价格。
- [ ] 完成单事务保存、提交后事件发送和无密钥 `AccountDto` 返回；提交前任一步失败均回滚且不发事件。
- [ ] 增加原子新增、原子编辑、映射/价格故障回滚、价格字段恢复、官方回退、禁用映射保留数据及账号类型限制测试。

## 验收标准
- [ ] 新增成功后账号、加密 credential、标签及完整同名映射同时存在，且没有账号价格覆盖记录。
- [ ] 编辑成功后基础字段、密钥、标签、映射和价格覆盖作为一个版本同时生效。
- [ ] 映射或价格写入失败时新增不留任何记录，编辑保持全部旧值，且不发送更新事件。
- [ ] 单字段或全字段恢复后继承官方价格，不创建空价格快照；字段级覆盖和官方回退语义保持不变。
- [ ] OAuth 或非法账号 ID 不能通过保存命令修改；成功返回值及事件不包含 API Key 明文。
- [ ] 新页面所需保存路径不依赖多个会自行提交的旧 mapping/price 命令。

## 验证步骤
- [ ] 运行 `accounts.rs` 原子新增、编辑及故障回滚测试。
- [ ] 运行 `pricing.rs` 字段覆盖、恢复、官方回退和禁用映射测试。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway` 并确认通过。

## 范围外事项
不删除旧兼容命令，不新增 migration，不实现 OAuth 新增、授权或可编辑保存。
