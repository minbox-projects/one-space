# Tauri 2 / Rust 2021 / rusqlite 0.31 数据库迁移库研究

研究日期：2026-08-04。范围仅限官方仓库、官方发布页和官方源代码；未读取项目代码。因此，涉及现有 `app_schema_migrations(subsystem, version)` 的步骤以该表确实记录当前数据库 schema 版本、且现有版本为 v1--v3 为前提。

## 结论

**首选：`refinery = { version = "0.9.2", features = ["rusqlite-bundled"] }`。**

它直接支持现有 `rusqlite::Connection`，官方依赖范围是 `>=0.23, <=0.39`，所以可以与 `rusqlite 0.31` 统一解析；`rusqlite-bundled` 特性会转发 `rusqlite/bundled`。迁移可由 `embed_migrations!` 编译进应用，保留独立 SQL 文件；有持久化历史表、确定性校验和、缺失/分歧校验和 `Target::FakeVersion` 基线能力。它是唯一一个同时满足“保持 rusqlite 架构”和“可把 v1--v3 记为已应用而不重跑 SQL”的候选方案。

**次选：`rusqlite_migration = "=1.2.0"`（不是当前 2.x）。**

版本 1.2.0 精确依赖 `rusqlite 0.31.0`，可与当前 bundled SQLite 组合，且 SQL 可用 `include_str!` 或 `from-directory`/`include_dir` 嵌入。它很小且原生同步，但状态只存 SQLite `PRAGMA user_version`，没有逐条历史、名称或 SQL 校验和，无法忠实接管 `(subsystem, version)` 的多子系统历史。仅在确认只需要一个全局线性版本、可接受放弃历史审计和漂移检测时采用。

**不建议：`barrel`、`sqlx migrate`、`diesel_migrations`。**

`barrel` 是生成 SQL 的 schema builder，不是执行/记录迁移的版本管理器，并且官方仓库最后提交为 2021-07-07；可作为 `refinery` 中 Rust 迁移的 SQL 生成辅助，但不应成为本项目的迁移基础设施。`sqlx migrate` 和 `diesel_migrations` 都有成熟的迁移能力，却要求改用各自的连接/ORM 架构；对一个既有 rusqlite 架构而言会引入第二套 SQLite 驱动、生命周期和 API，收益不足以覆盖迁移成本。

## 适配矩阵

| 方案 | rusqlite 0.31 bundled | Tauri 2 / Rust 2021 / 无 Java | SQL 文件嵌入 | 事务、校验和、历史 | SQLite 与并发/锁 | 维护状态（研究日） | 结论 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `refinery 0.9.2` | **兼容**：官方范围 `>=0.23, <=0.39`；使用 `rusqlite-bundled` | 完全原生 Rust；核心 crate edition 2021；不需要 Java | `embed_migrations!`；也可从目录运行 | 每条默认独立事务，或 `set_grouped(true)`；历史表含版本、名称、时间、checksum；默认拒绝缺失/分歧 | 支持 rusqlite/SQLite。未见 SQLite 专用进程间锁实现；需由应用在启动阶段单实例化迁移并配置 busy timeout/WAL | 最新 v0.9.2，2026-06-10 发布 | **首选** |
| `rusqlite_migration 1.2.0` | **兼容**：精确 `rusqlite 0.31.0`；其依赖默认未开 bundled，由根依赖统一开启即可 | 完全原生 Rust；edition 2021；MSRV 1.70；不需要 Java | `include_str!`，或 `from-directory` 特性将目录编译入二进制 | 所有向上/向下迁移分别在一个总事务中；仅 `user_version`，没有历史行或 checksum；可做 FK 检查 | SQLite 专用；未提供跨进程迁移锁，需应用协调 | 当前主线已到 2.6.0 且改依赖 rusqlite 0.40；1.2.0 是兼容 0.31 的历史固定版本 | **次选，仅接受功能降级时** |
| `barrel 0.6.6-alpha.0`（仓库 manifest） | 不依赖 rusqlite，生成 SQLite SQL 后仍要自行执行/记录 | 原生 Rust；不需要 Java | 不适用：它生成 SQL 字符串，不管理迁移文件 | 无运行器、事务、checksum 或历史表 | 有 SQLite 方言生成器；锁由调用者处理 | 官方仓库最新提交 2021-07-07，manifest 仍是 alpha | **不作为迁移库** |
| `sqlx migrate` | 不使用 rusqlite，而使用 SQLx 的 `SqliteConnection/Pool`；与现有 rusqlite 架构不直接兼容 | 原生 Rust；不需要 Java；但异步 SQLx 架构 | `sqlx::migrate!` 嵌入，CLI 也可从目录运行 | `_sqlx_migrations` 有 success、checksum、执行时长；默认事务；可逆 up/down；有 dirty 状态 | SQLite 实现 `lock()` 为空操作；默认事务。当前主线支持 `-- no-transaction`，但该能力须与所选 SQLx 版本核对 | 官方仓库活跃；本次官方 API 查询未返回 GitHub latest release 端点 | **不建议引入第二驱动/异步栈** |
| `diesel_migrations 2.3.x` | 不使用 `rusqlite::Connection`；要求 Diesel `Connection`/SQLite backend，不能直接复用 rusqlite | 原生 Rust；不需要 Java；但会引入 Diesel | `embed_migrations!`；目录为 `{version}_{name}/up.sql`、`down.sql` | 默认每条事务；可用 `metadata.toml` 关闭；追踪已执行迁移 | 支持 SQLite，经 Diesel 连接；锁行为随 Diesel/SQLite 连接实现，且会改变数据访问层 | Diesel v2.3.11 于 2026-07-10 发布 | **不建议，除非整个数据层迁至 Diesel** |

## 候选方案依据与限制

### 1. `refinery`（首选）

推荐依赖：

```toml
refinery = { version = "0.9.2", features = ["rusqlite-bundled"] }
```

- 官方 `refinery-core` manifest 中，`rusqlite-bundled = ["rusqlite", "rusqlite/bundled", "config"]`；`rusqlite` 约束是 `>= 0.23, <= 0.39`。这包含当前 `0.31`，Cargo 可以只解析一套 `rusqlite`/`libsqlite3-sys`。不要同时以不兼容的 SQLite native binding 特性再引入第二份 `links = "sqlite3"` 依赖。
- 官方 README 说明支持 `.sql` 或 Rust migration，命名为 `V{version}__{name}.sql`/`.rs`，且可通过 `embed_migrations!` 运行。对桌面应用，应嵌入 SQL，避免依赖应用安装目录旁的可变迁移文件。
- 源码计算 `name + version + SQL` 的 SipHash-1-3 checksum；默认 `abort_divergent = true`、`abort_missing = true`。默认历史表名为 `refinery_schema_history`，列为 `version`、`name`、`applied_on`、`checksum`。
- `Runner::set_grouped(false)` 是默认值，表示每条迁移单独事务；可改为 `true` 把一次升级中的全部 pending migration 包进单个事务。对于 SQLite schema 重建型迁移，保持每条原子通常更利于恢复；是否按批次原子化应由产品升级语义决定。
- 官方 README/源码没有给 rusqlite 运行器提供额外的跨进程迁移锁语义。因此不要依赖库来处理两个 Tauri 进程或两个连接同时启动。启动时应先停止/禁止其他数据库访问，在同一个写连接中设置 `busy_timeout`，按部署策略预先设置 WAL，然后运行迁移；仅在成功后初始化读写连接池/命令处理器。SQLite 同一时刻只有一个 writer，冲突时应重试或明确报告“数据库正在升级”，不能静默并发执行。

### 2. `rusqlite_migration`（次选）

推荐依赖仅限：

```toml
rusqlite_migration = "=1.2.0"
```

- 官方 v1.2.0 manifest 是 edition 2021，`rust-version = "1.70"`，依赖 `rusqlite = "0.31.0"`。注意 Cargo 的该版本要求等价于 `>=0.31.0, <0.32.0`，适配当前 0.31；不能跟随当前 2.6.0，因为其官方 manifest 已改为 rusqlite 0.40.0。
- `Migrations::to_latest` 把所有 pending up migration 和 `PRAGMA user_version` 更新置于一个事务；失败则回滚。可选择 `M::foreign_key_check()`；源码明确说明不要把 `foreign_keys` 或 `journal_mode` PRAGMA 放到 migration SQL 中，因为迁移位于事务内。
- 该设计的优点正是零迁移表；相应代价是数据库中只有一个整数，无法储存 `subsystem`、迁移名称、应用时间或 checksum，也无法发现已经发布的 SQL 被修改。其线性数组下标版本模型也不适合多个独立 subsystem。

### 3. `barrel`

官方 README 将它定义为“schema builder”，示例只创建 `Migration` 后调用 `make::<Sqlite>()` 获得 SQL；没有数据库连接、已应用历史或迁移执行 API。官方仓库 manifest 为 `0.6.6-alpha.0`，且 GitHub API 显示最后一次提交为 2021-07-07。即使 `refinery` README 说它可与 Barrel 配合，也只应在确有“以 Rust builder 生成 SQL”需求时作为可选开发依赖，不能解决本项目的版本管理与接管问题。

### 4. `sqlx migrate`

- 官方 SQLx 源码默认表为 `_sqlx_migrations`，SQLite 表含 `version`、`description`、`installed_on`、`success`、`checksum`、`execution_time`。执行时会先检测失败的 dirty migration，比较已应用 migration checksum，并在默认事务内把 SQL 与成功记录一起提交。
- `Migrator::set_locking(true)` 是默认设置，但官方 SQLite `Migrate::lock()` 实现返回 `Ok(())`，即没有额外锁。这不是对 SQLite 写锁的替代。
- 它可以通过 `sqlx::migrate!` embed SQL，CLI 也支持创建、运行、回滚。然而执行 API 要 `SqliteConnection` 或 pool，不接受 `rusqlite::Connection`。为了仅执行迁移而引入 SQLx，会让同一个数据库的连接配置、错误类型、同步/异步模型和 SQLite binding 来源分裂；并可能因两个 SQLite native binding 的 `links` 约束导致依赖解析问题。

### 5. `diesel_migrations`

官方 crate 文档明确它是 Diesel 的 schema maintenance API：迁移需要 Diesel `MigrationHarness`/connection，默认目录迁移每个版本均包含 `up.sql` 和 `down.sql`，并可嵌入到编译产物。它默认每条事务，也允许 `metadata.toml` 的 `run_in_transaction = false`。

这套能力不使用 `rusqlite::Connection`。采用它意味着为迁移单独引入 Diesel，或将数据库访问层迁移到 Diesel；两者都超出了单纯替换版本管理库的范围。因此即使其维护活跃，也不适合作为当前架构的局部改动。

## 从 `app_schema_migrations(subsystem, version)` v1--v3 接管到 refinery

### 前提与不变量

1. 先从旧表读取全部 `(subsystem, version)`，并确认目标 subsystem 的唯一最新值确为 `3`；若行缺失、存在多条冲突版本、或版本不是 1--3，停止启动并保留人工恢复路径，不能猜测 baseline。
2. 将旧 v1--v3 的 SQL 以不可变文件保存为 `V1__...sql`、`V2__...sql`、`V3__...sql`；新变化从 `V4__...sql` 起。对已存在数据库，这三条**不会执行**，但必须保留，因为 refinery 在后续启动时以它们验证历史 checksum。
3. 不把 refinery 的 migration table 名直接改为 `app_schema_migrations`。旧表的主键/列语义含 `subsystem`，而 refinery 的历史表是全局整数 `version`、`name`、`applied_on`、`checksum`；强行复用会丢失 checksum 并改变其他 subsystem 的记录语义。

### 一次性接管流程

1. 发布含 `V1`--`V4`（或仅到当前代码最高版本）的 embedded migration 集合的版本，但在第一次接管代码路径中，先禁止其他本地数据库连接执行读写。为迁移连接配置合理的 SQLite `busy_timeout`；WAL 和 `foreign_keys` 是连接级/部署级配置，分别在打开连接后按现有策略设置，不写入 transaction 内的 migration SQL。
2. 读取并验证旧表 v1--v3 与实际 schema 的关键不变量（例如必要表/列/索引存在）。这一步不能由历史表替代，因旧表不带 checksum。
3. 若 `refinery_schema_history` 不存在且旧记录确认目标 subsystem 已为 v3，调用 runner 的 `Target::FakeVersion(3)`：官方源码说明 Fake/FakeVersion 不执行 migration SQL，只创建/更新历史表。它会写入 v1--v3 对应的名称和 checksum，之后正常 `Runner::run` 将只执行 v4 及以后版本。
4. 接管成功后保留 `app_schema_migrations`，不要删除或改写；可增加一个仅用于审计的“已接管”标记，或以 `refinery_schema_history` 已含 v1--v3 作为幂等条件。后续所有新 schema 变更只用 refinery，不再写旧表。
5. 新安装（旧表不存在）直接正常运行同一 embedded runner，使 v1--v3（再到后续版本）在空库执行。升级安装走第 1--4 步。接管逻辑必须只在 history 表为空时运行；history 已存在时执行常规校验与迁移。
6. 为“空库”“v1、v2、v3 升级库”“旧表异常”“已接管后重启”“修改已发布 SQL”各建一项迁移测试。最后一项必须断言 refinery 因 checksum 分歧失败，而不是继续升级。

### 需要在实现前确认的项目事实

- `subsystem` 是否只有一个 schema 所有者；若有多个 subsystem，必须为每个 subsystem 设计独立的版本命名空间/独立 refinery history table，或先合并为一个全局 schema 升级序列。单个 refinery runner 的 version 是全局整数，不能直接表达复合键。
- v1--v3 的原始 SQL 是否仍可重建空数据库。若不能，先补齐经测试的历史 migration，再 baseline；不能只把当前 schema dump 伪装成三条历史 migration。
- 现有 `rusqlite` 依赖的精确特性和锁文件是否仍是 0.31 bundled。报告仅依据给定约束，未读取项目 manifest；实现时须用 Cargo 解析结果确认只保留一套 `libsqlite3-sys`。

## 官方来源

1. [`refinery` README：驱动支持、SQL/Rust migration、嵌入宏与命名](https://github.com/rust-db/refinery/blob/main/README.md)
2. [`refinery-core` 0.9.2 manifest：rusqlite `>=0.23, <=0.39` 与 bundled feature](https://github.com/rust-db/refinery/blob/main/refinery_core/Cargo.toml)
3. [`refinery` v0.9.2 官方发布页：2026-06-10 发布、支持 rusqlite 0.39](https://github.com/rust-db/refinery/releases/tag/v0.9.2)
4. [`refinery` Runner 源码：历史表名、grouped、缺失/分歧默认行为、Fake target](https://github.com/rust-db/refinery/blob/main/refinery_core/src/runner.rs)
5. [`refinery` traits 源码：历史表结构、checksum 验证、Fake/FakeVersion 写入行为](https://github.com/rust-db/refinery/blob/main/refinery_core/src/traits/mod.rs)
6. [`rusqlite_migration` v1.2.0 manifest：edition、MSRV、rusqlite 0.31](https://github.com/cljoly/rusqlite_migration/blob/v1.2.0/rusqlite_migration/Cargo.toml)
7. [`rusqlite_migration` 源码：`user_version`、事务、嵌入目录和 FK 检查](https://github.com/cljoly/rusqlite_migration/blob/v1.2.0/rusqlite_migration/src/lib.rs)
8. [`rusqlite_migration` 当前 manifest：2.6.0 使用 rusqlite 0.40](https://github.com/cljoly/rusqlite_migration/blob/master/Cargo.toml)
9. [`barrel` README：其 schema builder 定位](https://github.com/rust-db/barrel/blob/master/README.md)
10. [`barrel` manifest：0.6.6-alpha.0 与可用 backend](https://github.com/rust-db/barrel/blob/master/Cargo.toml)
11. [`barrel` 最新提交的官方 GitHub API 记录：2021-07-07](https://api.github.com/repos/rust-db/barrel/commits?per_page=1)
12. [`SQLx` migration runner：默认表、checksum 校验、锁开关与嵌入 API](https://github.com/launchbadge/sqlx/blob/main/sqlx-core/src/migrate/migrator.rs)
13. [`SQLx` SQLite migration 实现：表结构、无额外 lock、事务与 dirty record](https://github.com/launchbadge/sqlx/blob/main/sqlx-sqlite/src/migrate.rs)
14. [`SQLx CLI` README：建档、执行、回滚与目录 source](https://github.com/launchbadge/sqlx/blob/main/sqlx-cli/README.md)
15. [`diesel_migrations` 源码文档：目录/嵌入、事务与 metadata`](https://github.com/diesel-rs/diesel/blob/master/diesel_migrations/src/lib.rs)
16. [`diesel_migrations` manifest：对 Diesel 的直接依赖及 SQLite feature](https://github.com/diesel-rs/diesel/blob/master/diesel_migrations/Cargo.toml)
17. [`Diesel` v2.3.11 官方发布页：2026-07-10](https://github.com/diesel-rs/diesel/releases/tag/v2.3.11)
