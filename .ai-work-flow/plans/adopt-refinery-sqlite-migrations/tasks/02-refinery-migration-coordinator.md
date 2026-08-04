# 02 - 实现 Refinery 迁移协调器与旧库接管

- task_id: `refinery-migration-coordinator`
- order: `02`
- blocked_by: `dependency-migration-assets`
- source_plan: `../plan.md`
- source_plan_digest: `060dc325029ae1736a7ed8c01a8e12901bcc2ce9d5b304c611e43b6b4425dcd6`
- write_scope: `src-tauri/src/shared_sqlite/, src-tauri/src/ai_routing_gateway/storage.rs, src-tauri/src/ai_routing_gateway/commands/mod.rs`

## 预期结果

共享 SQLite 层通过单一迁移协调器完成空库迁移、可信 legacy 连续前缀的严格校验与 Refinery fake 接管，并在任何历史、结构、锁或执行异常时返回可观察的分类错误。

## 实施清单

- [ ] 用嵌入式 Refinery Runner 替换 `Migration`、`MIGRATIONS` 和手写逐版本执行循环，并使用独立、固定命名的 Refinery 历史表。
- [ ] 在迁移前取得 SQLite 写锁，沿用 5 秒 busy/locked 等待边界，保证多实例串行进入迁移路径。
- [ ] 对无历史的新库由 Refinery 执行 v1-v4，并保证每个版本使用独立事务。
- [ ] 探测 legacy `app_schema_migrations`，校验 subsystem、正版本、无重复、无缺号且不超过 v4 的连续前缀。
- [ ] 按 legacy 前缀验证对应表、列、索引、触发器、约束和必要种子；验证通过后在 Refinery 历史表登记已执行版本，再补齐剩余版本。
- [ ] 将 `app_schema_migrations` 保持为只读输入，接管和后续启动均不插入、更新或删除其记录。
- [ ] 启用 checksum 分歧、嵌入迁移缺失和未来版本严格拒绝；拒绝路径不得留下 Refinery 接管记录。
- [ ] 扩展 `SharedSqliteError`，区分锁超时、legacy 历史不可信、schema 漂移、未来版本、checksum/资源异常和迁移执行失败。
- [ ] 接入 `open/open_at` 及网关存储错误映射，使启动迁移完成后的普通连接不再次执行已完成迁移，同时保留底层诊断原因。
- [ ] 添加针对空库、一个代表性 legacy 前缀、重复打开、拒绝路径和单版本回滚的协调器测试。
- [ ] 完成本任务 checklist 更新。

## 验收标准

- [ ] 空库执行后具有完整 v1-v4 schema、基础种子和四条 Refinery 历史，legacy 表未被新增写入。
- [ ] 有效 legacy 连续前缀经结构验证后被准确登记，只执行缺失版本并保留业务数据。
- [ ] 缺号、重复、非正版本、未知 subsystem、超过 v4 或 schema 不符均在接管登记前失败。
- [ ] 已有 Refinery 历史的未来版本、checksum 分歧和缺失嵌入资源均被严格拒绝。
- [ ] 单版本失败时该版本 SQL 对象和历史记录完整回滚，已完成的较早版本保持一致。
- [ ] 并发迁移由写锁串行化，锁等待总边界不超过 5 秒，超时不会降级返回可用连接。
- [ ] 已完成迁移后的重复打开和 HTTP runtime 普通连接不会重复执行迁移 SQL。
- [ ] 网关调用链可记录错误分类和原因，但不会泄露 SQL 内容或业务数据。

## 验证步骤

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml shared_sqlite -- --nocapture`，预期协调器基础正向与负向测试通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway -- --nocapture`，预期存储错误映射和现有初始化测试通过。
- [ ] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`，预期所有共享数据库调用方编译通过。

## 范围外事项

不重排 Tauri 应用启动流程，不实现原生阻塞对话框，也不在本任务穷举全部兼容性夹具。
