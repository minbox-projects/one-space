# 01 - 建立共享存储与安全基座

- task_id: `ai-routing-storage-security-foundation`
- order: `01`
- blocked_by: `none`
- source_plan: `../plan.md`
- source_plan_digest: `4fd0f21d03c9fd3060e030ce14e81636333e3060e47dee01c6720c5983f985ce`
- write_scope: `src-tauri/Cargo.toml；src-tauri/Cargo.lock；src-tauri/src/lib.rs（仅注册 shared_sqlite 与 ai_routing_gateway 模块）；src-tauri/src/shared_sqlite/**；src-tauri/src/ai_routing_gateway/mod.rs；src-tauri/src/ai_routing_gateway/storage/{mod.rs,migrations.rs,schema.rs}；src-tauri/src/ai_routing_gateway/security.rs；src-tauri/src/ai_routing_gateway/types/{mod.rs,storage.rs}；对应 Rust 测试`

## Outcome

应用可事务化初始化共享 SQLite 和完整 `ai_gateway_` schema，并能通过 Keychain 根密钥安全加解密逐记录凭据；失败时网关保持可诊断的停止或锁定状态。

## Implementation Checklist

- [ ] 建立固定数据库路径、父目录创建、连接管理及迁移协调器。
- [ ] 启用 WAL、foreign keys 和明确的 busy timeout。
- [ ] 建立 `(subsystem, version)` 唯一的 `app_schema_migrations`。
- [ ] 在事务中创建计划列出的全部 `ai_gateway_` 表、约束、外键和索引。
- [ ] 建立 `keyring` 根密钥服务和 AES-256-GCM 版本化载荷。
- [ ] 使用随机 nonce，并将记录类型与稳定实体 ID 纳入 AAD。
- [ ] 区分全新初始化、Keychain 暂时不可用和已有密文但根密钥丢失。
- [ ] 将新模块骨架纳入 crate 编译，但不注册命令或启动运行时。

## Acceptance Criteria

- [ ] 数据库固定为 `~/.config/onespace/data/onespace.sqlite3`，网关 subsystem 固定为 `ai_routing_gateway`。
- [ ] schema 只创建 `app_schema_migrations` 和计划列出的 `ai_gateway_` 表，不修改未知表、现有 JSON/app_store 或 Protocol Router 数据。
- [ ] 默认组唯一性、必要外键、历史日志和聚合快照保留规则由数据库约束支持。
- [ ] 首建、升级、重复迁移、迁移失败回滚和两个初始化器并发竞争均得到唯一一致结果。
- [ ] 相同明文重复加密产生不同密文；AAD 不匹配、密文篡改和未知版本均拒绝解密。
- [ ] 已存在网关密文但根密钥缺失时不得生成替代根密钥，状态必须为锁定。
- [ ] 不读取或复用 `.local_key`。
- [ ] `src-tauri/src/lib.rs` 的本任务差异只有 `shared_sqlite` 与 `ai_routing_gateway` 两个模块声明，无初始化、命令注册或其他逻辑。

## Verification Steps

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml shared_sqlite`，迁移和连接测试全部通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::storage`，schema、回滚、并发和约束测试全部通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::security`，Keychain 替身和 AES-256-GCM 测试全部通过。
- [ ] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`，crate 编译通过。
- [ ] 检查 `git diff -- src-tauri/src/lib.rs`，确认只包含两个模块声明。

## Out of Scope

不实现账号业务、OAuth、额度刷新、网关 Key、HTTP 服务、Tauri 命令或前端功能。
