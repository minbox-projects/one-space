# 01 - 共享 SQLite、完整 Schema 与安全基座

- task_id: `task-01`
- order: `01`
- blocked_by: `none`
- source_plan: `../plan.md`
- source_plan_digest: `92f85a7f07acc328e48edf775eae5bfb751f58861b7c1b93de18fb68ed5fd822`
- write_scope: `src-tauri/src/shared_sqlite/；src-tauri/src/ai_routing_gateway/ 的模块骨架、存储连接边界、安全状态和基础错误类型；src-tauri/src/lib.rs；src-tauri/Cargo.toml`

## Outcome

应用具备可并发初始化、可前向迁移的共享 SQLite 基座，以及由 macOS Keychain 根密钥保护的逐记录 AES-256-GCM 凭据存储；失败时进入稳定锁定状态，网关服务仍保持关闭。

## Implementation Checklist

- [x] 唯一负责 `src-tauri/src/shared_sqlite/`、安全存储基础设施、初始完整 `ai_gateway_` schema、迁移执行器及相关测试替身。
- [x] 唯一负责 `src-tauri/Cargo.toml` 中本阶段所需的 SQLite、Keychain、加密依赖调整。
- [x] 唯一负责在 `src-tauri/src/lib.rs` 增加 `shared_sqlite` 与 `ai_routing_gateway` 模块声明；不得添加初始化或命令注册。
- [x] 初始迁移一次性建立计划要求的全部表、外键、删除语义、索引和默认设置/默认组，不拆成后续单表任务。
- [x] 建立 `src-tauri/src/ai_routing_gateway/` 的模块骨架、存储连接边界、安全状态和基础错误类型；不实现领域业务。

## Acceptance Criteria

- [x] 数据库固定创建于 `~/.config/onespace/data/onespace.sqlite3`，启用 WAL、foreign keys 和明确 busy timeout。
- [x] `app_schema_migrations` 以 `(subsystem, version)` 唯一，AI 子系统只写 `ai_routing_gateway` 版本，迁移失败不记录版本且完整回滚。
- [x] 首建、重复初始化、并发初始化、版本升级和存在未知未来表均不会重复建表、破坏数据或修改未知表。
- [x] 初始 schema 包含计划列出的全部 `ai_gateway_` 表、约束和查询索引，并正确保留历史快照。
- [x] AES-256-GCM 使用随机 nonce、cipher version 及“记录类型 + 稳定记录 ID”AAD；篡改、未知版本和 AAD 不匹配均使凭据不可用。
- [x] Keychain 仅在不存在既有网关密文时创建根密钥；已有密文但 Keychain 缺失或不可访问时进入锁定且不覆盖旧密文。
- [x] 日志和错误只暴露安全类别与实体 ID，不输出密钥、token、nonce 对应明文或完整正文。
- [x] Rust 测试覆盖迁移原子性、PRAGMA、并发初始化、Keychain 替身、根密钥丢失、AES nonce/AAD/篡改/版本和脱敏。

## Verification Steps

- [x] 执行本任务 Acceptance Criteria 对应的 Rust 存储与安全测试并确认全部通过。

## Out of Scope

不修改 `run_app.rs`、Protocol Router、JSON 或 app_store 数据源；不实现领域业务。
