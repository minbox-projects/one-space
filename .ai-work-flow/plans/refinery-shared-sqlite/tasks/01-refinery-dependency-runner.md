# 01 - 建立 Refinery 唯一迁移 Runner 与旧库兼容桥接

- task_id: `refinery-dependency-runner`
- order: `01`
- blocked_by: `none`
- source_plan: `../plan.md`
- source_plan_digest: `50b29d6d6bcdd89a9825522ddc3f8375de5427bb3bfc5b3fe58734e81585ae7c`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src-tauri/Cargo.toml`
  - `src-tauri/Cargo.lock`
  - `src-tauri/src/shared_sqlite/mod.rs`
  - `src-tauri/src/shared_sqlite/migrations.rs`
  - `src-tauri/src/shared_sqlite/migrations/`
  - `src-tauri/src/ai_routing_gateway/schema_v1.sql`
  - `src-tauri/src/ai_routing_gateway/schema_v2.sql`
  - `src-tauri/src/ai_routing_gateway/schema_v3.sql`
  - `src-tauri/src/ai_routing_gateway/schema_v4.sql`

## 预期结果

核验与 Rust 1.77.2、现有 rusqlite 0.31 bundled 兼容的最小 Refinery 依赖、feature、嵌入宏、Runner API 及事务能力；将旧 v1-v4 SQL 按稳定的一对一版本映射组织为唯一嵌入迁移资产，并以 shared_sqlite 单一入口取代旧执行路径。实现旧历史读取、完整 schema 指纹、状态分类、连续基线登记和剩余迁移执行，仅接受全新库、可验证的旧 v1-v4 库及一致的已桥接库；拒绝历史缺口、未来版本、subsystem 混淆、部分 schema 和历史矛盾，不猜测版本或重放已应用 SQL。保证基线与后续迁移原子提交、失败回滚、重复调用幂等，并保持数据库路径、连接 flags、PRAGMA、超时、BUSY/LOCKED 重试和按操作开库行为不变。

## 实施清单

- [ ] 依据实施时的官方 Refinery 与 Cargo 文档核验 Rust 1.77.2、`rusqlite 0.31` bundled、最小 feature、嵌入迁移宏及使用现有 `rusqlite::Connection` 的 Runner API；确认依赖解析不会引入第二套 SQLite 链接实现。
- [ ] 用最小可编译调用验证 Refinery 的事务控制能力及其与显式 `IMMEDIATE` 边界的兼容性；若无法证明基线登记和剩余迁移原子提交，则停止实施并报告，不降级为逐条提交。
- [ ] 将既有 v1-v4 SQL 按 Refinery 约定组织到 shared SQLite 唯一嵌入迁移目录，保持 SQL 语义和版本映射 1:1；删除迁移资产双源引用，使其他模块不能直接执行网关 schema SQL。
- [ ] 用 Refinery 单一迁移集合与 Runner 替换旧 `Migration` 数组、逐条 SQL 执行及新旧历史双重驱动逻辑；`app_schema_migrations` 仅作为一次性兼容识别输入。
- [ ] 为 v1-v4 定义可测试的完整 schema 指纹，覆盖必要表、列、索引、约束和迁移特有数据条件，并读取、校验 `ai_routing_gateway` 旧历史的连续性、唯一性、范围及 subsystem 归属。
- [ ] 明确分类全新库、带连续旧历史的 v1-v4 库、无旧历史但具有唯一完整指纹的 v1-v4 库和已桥接库；拒绝缺口、重复、未来版本、subsystem 混淆、部分 schema、错误索引及历史与实际 schema 矛盾。
- [ ] 在同一受控 `IMMEDIATE` 事务中登记从 v1 到识别版本的连续 Refinery 基线并仅执行剩余迁移；任何检查、基线、执行或提交失败均回滚事务内历史、schema 和数据改动。
- [ ] 保持 `shared_sqlite::open/open_at` 的固定路径、`READ_WRITE | CREATE | FULL_MUTEX`、5000ms busy timeout、WAL、`foreign_keys=ON`、BUSY/LOCKED 重试及按操作打开连接行为。
- [ ] 添加隔离临时 SQLite fixture，覆盖全新库、每个旧 v1-v4 状态、有历史与无历史桥接、已桥接库、非法状态、重复调用和故障回滚。

## 验收标准

- [ ] Refinery 是 AI 路由网关 schema 的唯一迁移 Runner 与权威执行历史，v1-v4 映射稳定、连续、嵌入式且无 SQL 双源。
- [ ] 新库得到完整表、列、索引、约束和默认数据，Refinery 历史仅包含连续 v1-v4，重复运行不重复默认数据或迁移记录。
- [ ] 真实 v1、v2、v3、v4 fixture 只执行缺失版本，既有业务数据、默认值和密钥语义保持不变，已应用 SQL 不被重放。
- [ ] 无旧历史库仅在唯一完整 schema 指纹匹配时建立基线；缺列、错误索引、部分 schema、历史缺口、未来版本、subsystem 混淆及历史矛盾均可诊断失败且无副作用。
- [ ] 基线登记与剩余迁移在同一受控事务中原子提交，注入失败后 Refinery 历史、旧历史、schema 和数据均无部分提交。
- [ ] 数据库路径、连接 flags、busy timeout、WAL、foreign keys、BUSY/LOCKED 重试及按操作开库模型无回归。

## 验证步骤

- [ ] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`，预期最小 Refinery 集成在项目固定 Rust 工具链下编译通过。
- [ ] 运行 `cargo tree --manifest-path src-tauri/Cargo.toml -i rusqlite`，预期依赖图只解析到兼容的单一 `rusqlite`/SQLite 链接实现。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml shared_sqlite`，预期新库、v1-v4 桥接、无历史指纹、矛盾状态、幂等与回滚测试全部通过。
- [ ] 检查迁移目录和引用点，预期只有 shared SQLite 模块嵌入并执行网关 schema，旧 SQL 文件不存在并行执行入口。
- [ ] 运行 `cargo fmt --manifest-path src-tauri/Cargo.toml --check`，预期格式检查通过。

## 风险提示

- [ ] Refinery 的 rusqlite 适配版本或事务封装可能与现有依赖及显式 `IMMEDIATE` 边界冲突；必须先完成依赖图和原子性验证，再锁定依赖或实现桥接。
- [ ] v2 表重建、v3 数据清理和 v4 `ALTER TABLE` 不可对已存在 schema 重放；任何无法唯一识别的状态都必须停止迁移。
- [ ] 已发布的 v1-v4 迁移内容和版本号在建立 Refinery 历史后不可改写，后续 schema 演进只能新增迁移。

## 范围外事项

- `app_store` JSON 或加密文件存储 SQLite 化及其 onboarding 流程调整。
- 外部 OpenCode SQLite 的路径、schema、迁移、读取或生命周期管理。
- 引入连接池、长期连接、共享连接所有权或修改 AI 路由网关业务 schema。
- 自动 schema 降级、自动删除历史或对无法识别的旧库执行补救性 SQL。
