# 04 - 完成迁移兼容性矩阵与全量验证

- task_id: `migration-compatibility-validation`
- order: `04`
- blocked_by: `startup-migration-gate`
- source_plan: `../plan.md`
- source_plan_digest: `060dc325029ae1736a7ed8c01a8e12901bcc2ce9d5b304c611e43b6b4425dcd6`
- write_scope: `src-tauri/src/shared_sqlite/, src-tauri/src/app_runtime/run_app.rs, src-tauri/src/ai_routing_gateway/, Rust test fixtures and test-only helpers`

## 预期结果

完整自动化测试证明新库、legacy v1-v4 接管、严格拒绝、独立事务、并发锁和应用启动门禁均符合规格，且 crate 的格式、编译和全量测试通过。

## 实施清单

- [ ] 重写旧迁移器专属断言，建立可复用但不过度抽象的空库、legacy 历史、schema 漂移和 Refinery 历史夹具。
- [ ] 覆盖空库执行 v1-v4、种子数据、独立 Refinery 历史及 legacy 表只读行为。
- [ ] 分别覆盖 legacy v1、v2、v3、v4 连续前缀接管，断言业务行保留、只补齐缺失版本、登记历史准确且重复启动幂等。
- [ ] 覆盖 legacy 缺号、重复、非正版本、未知版本、未知 subsystem，以及缺表、列、索引、触发器、约束或必要种子的严格拒绝。
- [ ] 覆盖未来 Refinery 版本、checksum 分歧、历史引用但嵌入资源缺失的拒绝，并断言消费者访问和服务启动均未发生。
- [ ] 保留未知业务表不受影响的兼容断言，证明验证器不会把无关私有对象当作接管依据或删除目标。
- [ ] 覆盖单版本失败回滚，断言 SQL 对象和 Refinery 历史均无半完成状态。
- [ ] 覆盖多连接并发 bootstrap、单一执行路径、5 秒写锁等待上限及超时分类。
- [ ] 覆盖应用门禁失败短路与成功初始化顺序，并验证原生错误报告边界无需图形环境。
- [ ] 审核最终依赖树，执行格式检查、专项测试、完整测试和编译检查，修复仅与本计划有关的回归。
- [ ] 按计划执行空库与已接管库的 Tauri 启动烟测并记录结果。
- [ ] 完成本任务 checklist 更新。

## 验收标准

- [ ] 空库和 legacy v1-v4 五类正向路径全部形成一致的最新 schema 与 Refinery v1-v4 历史。
- [ ] legacy 业务数据及未知业务表保持不变，`app_schema_migrations` 在接管前后逐行一致。
- [ ] 所有非法 legacy 历史、schema 漂移、future/checksum/missing 情形均在消费者和服务开放前失败，且不写入接管历史。
- [ ] 重复启动不重复执行迁移；并发启动最多一个执行路径，其他连接成功等待或在 5 秒边界分类失败。
- [ ] 单版本事务失败不留下该版本对象或历史记录。
- [ ] 启动门禁成功和失败生命周期断言均通过，失败报告不泄露敏感内容。
- [ ] Refinery/rusqlite 依赖树符合锁定方案，格式、专项测试、完整测试和编译检查全部通过。
- [ ] 空库和已接管库的应用启动烟测均通过。

## 验证步骤

- [ ] 运行 `cargo tree --manifest-path src-tauri/Cargo.toml`，预期 Refinery/rusqlite/libsqlite3-sys 解析符合锁定方案。
- [ ] 运行 `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`，预期无格式差异。
- [ ] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`，预期编译通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml shared_sqlite -- --nocapture`，预期迁移矩阵全部通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway -- --nocapture`，预期网关存储与生命周期回归通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml`，预期完整 Rust 测试通过。
- [ ] 按 README 运行 `npm run tauri dev`，分别使用空库和有效 legacy 库，预期均通过迁移门禁并正常启动。

## 范围外事项

不新增迁移 CLI、自动备份、逆迁移、未知私有 schema 兼容规则、前端业务功能或未在规格中定义的发布自动化。
