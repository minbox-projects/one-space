# Refinery 共享 SQLite 迁移实施计划

## 计划元数据

- plan-id: `refinery-shared-sqlite`
- status: `ready-for-implementation`
- source_spec: `.ai-work-flow/plans/refinery-shared-sqlite/spec.md`
- source_spec_digest: `cdd2ef8582ece08b5f8e67e24eed0fda83c300c39a090f1e92cb920dd7057ae4`
- task_mode: `split`

## 技术与代码上下文

- 当前唯一共享库入口为 `src-tauri/src/shared_sqlite/mod.rs::open/open_at`：固定 `$HOME/.config/onespace/data/onespace.sqlite3`，使用 `READ_WRITE | CREATE | FULL_MUTEX`，设置 5 秒 busy timeout、WAL 与 `foreign_keys=ON`，再调用 `migrations::apply`。
- 现有 `src-tauri/src/shared_sqlite/migrations.rs` 按单版本 `Immediate` 事务执行 `schema_v1.sql` 至 `schema_v4.sql`，并写入 `app_schema_migrations(subsystem, version)`；该机制会将缺失历史当作未执行，不能直接用于已存在 schema 的安全升级。
- `schema_v2.sql` 含表重建，`schema_v3.sql` 含数据清理，`schema_v4.sql` 含 `ALTER TABLE`；对已存在 schema 重新执行会破坏性失败，因此旧库必须先完成基线桥接。
- `src-tauri/src/ai_routing_gateway/storage.rs::open` 已经委托共享 SQLite；`commands::prepare_startup` 将成功打开记录为 `database_migrations`，`run_app.rs` 当前异步调用 `ai_routing_gateway::commands::initialize`。`app_store::ensure_migrated_on_startup` 位于独立 onboarding 分支，外部 OpenCode 读取位于 `ai_sessions`/`app_store` 路径。

## 实施方案

以 Refinery 管理 AI 路由网关 schema 的全部未来迁移，保留 v1-v4 SQL 的语义并映射为稳定、连续的嵌入式迁移版本。共享 SQLite 层暴露一个唯一 bootstrap/迁移入口：打开并完成现有连接配置后，先在受控 `IMMEDIATE` 事务中识别旧状态、核验实际 schema、必要时向 Refinery 历史表写入连续基线，随后由同一入口执行未应用的 Refinery 迁移。

桥接只接受三类确定状态：全新库、可验证且连续的旧 v1-v4 库、已桥接的 Refinery 库。历史缺口、未来版本、subsystem 混淆、Refinery 历史与实际 schema 矛盾、或不完整的无历史 schema 一律诊断失败；不猜测版本、不执行补救性 SQL、不重放 v1-v4。迁移完成是网关初始化的前置边界，但不改变 app_store onboarding、OpenCode 读取或按操作打开连接的模型。

## 顺序执行步骤

1. 核验依赖集成方案：依据实施时官方 Refinery/Cargo 文档确认与 Rust 1.77.2、现有 `rusqlite 0.31`（`bundled`）兼容的最小 `refinery` feature 组合、嵌入式 migration 宏与 `rusqlite::Connection` runner API；用临时最小调用执行 `cargo check` 与 `cargo tree -i rusqlite`。不在核验前锁定版本，也不引入额外 SQLite 链接实现。验证：单一 rusqlite 依赖图、最小 feature 可编译、迁移 runner 可使用现有连接。
2. 建立 Refinery 迁移资产与唯一 runner：将 v1-v4 SQL 按 Refinery 约定重命名/移动到仅由 shared SQLite 模块嵌入的迁移目录，保持 SQL 字节语义与版本映射 1:1；以 `embed_migrations` 或核验后的等价 API 定义单一迁移集合。移除旧 `Migration` 数组与逐条 SQL 执行路径，禁止其他模块直接运行网关 schema SQL。验证：新库执行后表、索引、约束和默认数据与现有 schema 契约一致，Refinery 历史仅含连续版本 1-4。
3. 实现旧历史识别与 schema 指纹：在 shared SQLite migration 模块内为每个 v1-v4 定义可测试的必要表、列、索引、约束及迁移特有数据条件，查询 `sqlite_master`、`PRAGMA table_info/index_list/index_xinfo` 和必要数据状态。读取 `app_schema_migrations` 的 `ai_routing_gateway` 记录，并拒绝重复/缺口/超范围记录、其他 subsystem 冒充网关记录，以及历史声明与 schema 指纹不匹配。验证：各历史 fixture 精确分类；仅有表名、缺列、错误索引或不一致数据的 fixture 不会被接受。
4. 实现事务化基线桥接与 Refinery 执行：先检查 Refinery 历史；若不存在，使用 `IMMEDIATE` 事务核验旧历史与实际 schema，或仅以完整 schema 指纹识别无旧历史库，然后向 Refinery 历史表登记从 1 至识别版本的连续基线，并在同一受控边界执行剩余迁移。已桥接库只验证 Refinery 历史连续性及对应实际 schema，再执行未来迁移；任一步失败回滚基线登记和 SQL 改动。旧 `app_schema_migrations` 只读作一次性桥接输入，不再写入或驱动执行。验证：每个 v1-v4 fixture 仅执行缺失版本，失败时 Refinery/旧历史和 schema 均无部分提交。
5. 接入启动编排：将显式的共享数据库 bootstrap 放在 `run_app` 网关初始化之前，成功后才 spawn/调用 `ai_routing_gateway::commands::initialize`；迁移失败记录包含阶段、数据库路径、识别/目标版本与底层原因的非敏感诊断，并阻断网关初始化和运行时暴露。保留 `storage::open`/`open_at` 的按操作调用幂等性，避免为迁移引入长生命周期连接或连接池；保持 app_store 的原有 onboarding 条件、执行位置和忽略错误语义。验证：启动序列测试证明失败时 initialize 未执行，成功时迁移完成先于网关准备；app_store 与 OpenCode 相关路径不发生调用或行为变化。
6. 完成回归矩阵、运行格式化和目标测试：为兼容桥接和启动阻断添加隔离的临时 SQLite fixture；执行 scoped Rust tests、全量相关 crate tests 及 `cargo fmt --check`。验证：下述矩阵通过，且不出现对共享路径、flags、PRAGMA 或 busy/locked 重试行为的回归。

## 任务边界与依赖

| 建议任务 | 边界 | 依赖 | 完成验证 |
| --- | --- | --- | --- |
| 1. 依赖与嵌入迁移 | Cargo 核验、Refinery 依赖、v1-v4 嵌入资产、唯一 runner 骨架 | 无 | `cargo check`、新库 schema 契约 |
| 2. 兼容桥接 | 旧历史读取、schema 指纹、事务基线与错误分类 | 1 | v1-v4、无历史、矛盾状态与回滚 fixture |
| 3. 启动门控 | `run_app` 编排、诊断传播、网关阻断测试 | 1、2 | 迁移成功先初始化、失败不初始化 |
| 4. 回归加固 | 并发、PRAGMA、范围保护和全量相关测试 | 1、2、3 | 测试矩阵及范围检查 |

任务间只通过已确定的 runner 接口、Refinery 版本映射和诊断错误契约衔接；不要在本计划阶段创建任务文件。任务 2 不得在任务 1 的依赖/API 核验未通过前假定 Refinery 历史表名或写入格式。

## 具体改动

| 候选文件 | 改动 |
| --- | --- |
| `src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` | 在核验后加入最小 Refinery 依赖/feature；锁文件仅随 Cargo 正常解析更新。 |
| `src-tauri/src/shared_sqlite/migrations.rs` | 用嵌入式 Refinery migration 定义、状态识别、schema 指纹、基线桥接、唯一 runner 和阶段化错误替代自定义逐条执行器。 |
| `src-tauri/src/shared_sqlite/mod.rs` | 保持连接配置不变，调整为在配置后调用唯一 runner；扩展测试 fixture 与回归断言。 |
| `src-tauri/src/shared_sqlite/migrations/V1__*.sql` 至 `V4__*.sql`（或核验后的 Refinery 目录命名） | 承载现有 v1-v4 SQL，保持顺序、内容和版本映射。 |
| `src-tauri/src/ai_routing_gateway/schema_v1.sql` 至 `schema_v4.sql` | 在迁移资产移动后删除旧副本或改为唯一嵌入资产，避免双源 SQL；不改业务 SQL 语义。 |
| `src-tauri/src/app_runtime/run_app.rs` | 在网关异步初始化前建立显式迁移成功边界和失败诊断；保持 app_store 分支不变。 |
| `src-tauri/src/ai_routing_gateway/commands/mod.rs`、`storage.rs`（仅在接口需要时） | 适配预迁移完成状态及可诊断错误，保留已有网关生命周期与每操作开库行为。 |

## 接口与数据流

`run_app::setup` -> `shared_sqlite::bootstrap_gateway_schema`（打开固定共享库、既有 flags/PRAGMA、状态检查/桥接、Refinery 执行、提交）-> 成功 -> `ai_routing_gateway::commands::initialize` -> `storage::open`/后续请求继续按操作调用 `shared_sqlite::open`。

状态分支与不变量：

- 全新库：无网关对象、无 Refinery 历史；仅 Refinery 从 v1 顺序执行至最新版本，默认数据只由所属迁移产生一次。
- 已有 v1-v4 库：旧历史与完整对应 schema 指纹一致，或无旧历史但完整指纹唯一匹配；在同一 `IMMEDIATE` 事务登记连续 Refinery 基线后，只运行剩余版本。
- 已桥接库：Refinery 历史连续且与实际 schema 匹配；不写旧历史、不重放已登记版本，只运行新增版本。
- 任何不一致：不登记、不执行、不初始化网关。历史表不是唯一证据，schema 指纹也不能接受部分状态。

Refinery 历史为执行权威；`app_schema_migrations` 保留兼容读取价值但不再成为新迁移执行记录。错误数据不包含凭据、密钥、token 或业务记录内容。

## 失败处理

- 将错误区分为 `check`、`baseline`、`execute`、`commit` 阶段，并附路径、识别旧版本/目标版本、底层 rusqlite 或 Refinery 原因；对外映射保持网关存储不可用语义，诊断只在安全的生命周期状态/日志中出现。
- 获取写锁、WAL 或执行迁移遇到 BUSY/LOCKED 时，复用当前 5 秒超时及重试语义；超时后失败并阻断初始化，不创建第二套锁或长期持有连接。
- 基线登记和剩余迁移必须由同一受控事务原子提交；任何 SQL、历史写入或提交失败均回滚该边界内改动。若 Refinery API 自带事务边界与显式 `IMMEDIATE` 不能兼容，实施任务先验证其事务控制能力；不能证明原子性则停止并报告，不降级为逐条提交。
- 发布前准备现有 v1-v4 fixture 的只读备份验证；生产失败的回滚方式为恢复数据库备份或回退应用二进制，不执行自动 schema 降级或删除 Refinery 历史。

## 测试与验证

| 场景 | 断言 |
| --- | --- |
| 全新库 | Refinery 建立完整表/索引/约束、默认 settings/default group，历史连续且无重复默认数据。 |
| v1、v2、v3、v4 各升级点 | 先由旧 SQL 构造真实旧库和业务数据，再桥接并升级；已应用版本不重放，数据与敏感字段既有语义保持。 |
| 无旧历史但完整 v1-v4 schema | 只在唯一完整指纹匹配时登记连续基线并升级。 |
| 历史/实际 schema 不一致 | 缺列、错误索引、版本缺口、未来版本、混淆 subsystem、部分 schema 均失败；没有 Refinery 基线或 schema 副作用。 |
| 幂等 | 重复 `open_at`、bootstrap 与应用启动不改变业务数据、默认数据和迁移记录计数。 |
| 失败回滚 | 注入基线写入或后续迁移失败，确认 schema、Refinery 历史及旧历史无部分提交，错误包含阶段与非敏感原因。 |
| 并发启动 | 多线程/多连接竞争同一临时库，最终仅一份连续 Refinery 历史和完整 schema，无 BUSY/LOCKED 漏处理。 |
| 启动阻断 | 模拟 bootstrap 失败，assert 网关 initialize/runtime 未启动且可读取诊断；成功路径顺序相反。 |
| PRAGMA 与连接回归 | 固定路径、三项 open flags、5000ms busy timeout、WAL、foreign keys 及 busy/locked 重试保持。 |
| 范围保护 | app_store onboarding 与文件迁移测试不变；OpenCode 读取测试仍使用其原路径/连接；确认没有引入池化或长期连接。 |

建议执行：先运行 shared SQLite 模块测试与 gateway lifecycle 测试，再运行 `cargo test`（按仓库既有命令）及 `cargo fmt --check`；依赖改动后额外运行 `cargo check` 和 `cargo tree -i rusqlite`。

## 验收标准

- Refinery 是 AI 路由网关 schema 的唯一迁移 runner 与权威历史；v1-v4 映射稳定、嵌入式且可追踪。
- 新库、每个受支持旧版本和已桥接库都得到完整且幂等的最新 schema；不重放已存在的 v1-v4 SQL。
- 只有已验证的连续历史与完整 schema 才能桥接；所有不确定状态可诊断地停止。
- 迁移在网关初始化前成功完成；失败不暴露网关运行时能力。
- 数据库位置、连接 flags、5 秒 timeout、WAL、foreign keys、BUSY/LOCKED 重试、按操作打开连接、app_store 和 OpenCode 行为均无回归。

## 兼容、迁移与发布

仅支持共享 `onespace.sqlite3` 的 v1-v4 网关状态。首个含 Refinery 的发行版在用户原地数据库上执行一次兼容桥接；之后所有网关 schema 演进仅添加新的 Refinery migration，禁止改写已发布迁移文件或修改其版本号。发布候选必须用 v1-v4 备份副本验证升级并保留可恢复备份；没有自动降级路径。app_store 与外部 OpenCode 不在本迁移的兼容或发布范围内。
