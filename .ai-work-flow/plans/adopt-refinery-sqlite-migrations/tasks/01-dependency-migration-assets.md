# 01 - 锁定 Refinery 依赖与嵌入迁移资源

- task_id: `dependency-migration-assets`
- order: `01`
- blocked_by: `none`
- source_plan: `../plan.md`
- source_plan_digest: `060dc325029ae1736a7ed8c01a8e12901bcc2ce9d5b304c611e43b6b4425dcd6`
- write_scope: `src-tauri/Cargo.toml, src-tauri/Cargo.lock, src-tauri/migrations/, src-tauri/src/ai_routing_gateway/schema_v*.sql, src-tauri/src/shared_sqlite/migrations.rs`

## 预期结果

项目使用兼容且唯一的 Refinery/rusqlite/bundled SQLite 依赖组合，并以 Refinery 规范命名、编译期嵌入且内容不变的 v1-v4 SQL 作为唯一迁移资源。

## 实施清单

- [ ] 选择支持当前 Rust 1.77.2 的 Refinery 与 rusqlite 兼容版本，启用嵌入迁移和 rusqlite 支持功能，并保留 `bundled` SQLite。
- [ ] 由 Cargo 更新锁文件，确认依赖图不存在不兼容或重复的 rusqlite/SQLite 绑定组合。
- [ ] 创建 `src-tauri/migrations/V1__ai_routing_gateway.sql` 至 `V4__ai_routing_gateway.sql`，逐字保留已发布 schema v1-v4 内容和 v1 种子数据。
- [ ] 接入 `refinery::embed_migrations!` 的最小编译入口，删除或停止引用旧 `include_str!` 注册表，确保只有一套可执行迁移资源。
- [ ] 添加或调整最小编译级测试，证明四个版本均被嵌入且版本、名称顺序正确。
- [ ] 完成本任务 checklist 更新。

## 验收标准

- [ ] `Cargo.lock` 包含 Refinery，rusqlite 版本与 Refinery 驱动兼容且继续启用 bundled SQLite。
- [ ] `cargo tree` 显示单一可用的 rusqlite/libsqlite3-sys 解析组合。
- [ ] v1-v4 新迁移文件与变更前对应 SQL 内容逐字一致，包含原有种子数据。
- [ ] 编译产物不依赖运行时外置 SQL 文件，旧路径不再构成第二套可执行来源。
- [ ] 嵌入迁移清单恰好包含全局版本 1、2、3、4。

## 验证步骤

- [ ] 运行 `cargo tree --manifest-path src-tauri/Cargo.toml`，审核 Refinery、rusqlite 与 libsqlite3-sys 解析结果。
- [ ] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`，预期嵌入迁移宏和依赖组合编译成功。
- [ ] 运行迁移资源最小测试，预期只发现按顺序排列的 v1-v4。

## 范围外事项

不实现历史接管、Runner 执行、应用启动门禁或完整兼容性测试矩阵。
