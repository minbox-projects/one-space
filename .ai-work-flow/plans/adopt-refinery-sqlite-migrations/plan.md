# 采用 Refinery 管理 SQLite 迁移实施计划

## 计划元数据

- plan-id: `adopt-refinery-sqlite-migrations`
- status: `ready-for-implementation`
- source_spec: `.ai-work-flow/plans/adopt-refinery-sqlite-migrations/spec.md`
- source_spec_digest: `03dacad722fc2e9d84bae566d52d085395a743994727507c61985e784564e688`
- task_mode: `split`

## 技术与代码上下文

- Rust 后端位于 `src-tauri`，使用 Rust 2021、最低 Rust `1.77.2`；当前 `rusqlite = 0.31` 启用 `bundled`。`Cargo.lock` 已锁定该版本，尚未包含 Refinery。
- 共享数据库入口为 `src-tauri/src/shared_sqlite/mod.rs` 的 `open` 与 `open_at`：它创建数据库目录，以读写、创建和全互斥标志打开 SQLite，配置 5 秒 busy timeout、WAL 与外键，再调用自定义迁移器。
- 自定义迁移器在 `src-tauri/src/shared_sqlite/migrations.rs`。它以 `MIGRATIONS` 注册 `schema_v1.sql` 至 `schema_v4.sql`，对每个版本使用 `BEGIN IMMEDIATE` 事务，并写入 `app_schema_migrations(subsystem, version, applied_at)`；唯一现有子系统为 `ai_routing_gateway`。
- 网关存储层 `src-tauri/src/ai_routing_gateway/storage.rs` 将共享数据库错误压缩为 `StorageUnavailable`。网关 `prepare_startup` 先打开数据库并记录 `database_migrations`，随后读取安全材料和设置；仅在此后才启动 HTTP runtime 和 scheduler。
- 应用级 `src-tauri/src/app_runtime/run_app.rs` 的 `setup` 当前会立即派生协议路由、网关初始化、会话同步、AI scheduler、SSH 监控及其他后台工作；该处尚无全局迁移失败即显示原生对话框并退出的门禁。
- `shared_sqlite` 单元测试已覆盖空库、并发 bootstrap、单迁移回滚、前向升级和 v1-v4 的业务升级夹具。网关、定价、请求日志、账户、配额和 runtime 测试普遍通过 `shared_sqlite::open_at` 构造临时数据库。

## 实施方案

- 在 `Cargo.toml` 升级 `rusqlite` 至与所选 Refinery SQLite 驱动兼容的版本，保留 `bundled`；新增 Refinery 的嵌入迁移与 rusqlite 支持功能，并更新锁文件。以 `cargo tree` 与编译结果确认单一可用 SQLite/rusqlite 依赖组合。
- 将四份已发布 SQL 以 Refinery 规范的全局版本文件名置于 `src-tauri/migrations/`，例如 `V1__ai_routing_gateway.sql` 至 `V4__ai_routing_gateway.sql`；文件内容逐字保持现有 SQL，不引入运行时外部资源。`shared_sqlite/migrations.rs` 使用 `refinery::embed_migrations!` 将该目录编译进二进制。
- 用一个迁移协调器替换 `Migration`、`MIGRATIONS` 与手写逐版本执行逻辑。协调器在获取 SQLite 写锁后先判定历史来源：已有 Refinery 历史时执行 Refinery 的严格校验和升级；仅有旧历史时完成接管校验、在 Refinery 历史表登记已验证的连续前缀，再执行剩余版本；两者均无时由 Refinery 从 v1 执行。
- 将 Refinery 历史表明确命名为独立表（例如 `refinery_schema_history`），绝不写入或删除 `app_schema_migrations`。配置 Refinery 的版本、校验和与缺失历史检查，使发布过的迁移资源与已登记历史不一致时失败而非继续。
- 把数据库迁移提升为应用启动的同步前置条件：`run_app::setup` 在启动 IPC 关联服务、HTTP runtime 与任何后台任务前调用共享数据库启动入口。失败时记录含底层分类和原因的日志，显示原生阻塞错误对话框，调用应用退出；成功后才注册或派生现有服务。网关的 `prepare_startup` 保持再次打开数据库的行为，但不会重新执行已完成迁移。

## 顺序执行步骤

1. 确定 Refinery 与 rusqlite 的兼容版本和 Cargo feature 集，更新 `Cargo.toml`、再由 Cargo 生成 `Cargo.lock`；先完成最小嵌入迁移编译检查。
2. 创建 Refinery 命名的 v1-v4 嵌入迁移文件，并将当前四份 schema SQL 原样迁移到新位置；移除旧的 `include_str!` 注册表，避免同一版本出现两套可执行来源。
3. 在 `shared_sqlite::migrations` 实现历史探测、旧历史与 schema 的前缀验证、Refinery 历史登记和 Runner 执行；保持每个迁移版本的独立事务与 5 秒写锁等待语义。
4. 扩展 `SharedSqliteError` 为可区分的启动阻塞类别，并让 `open_at` 在连接配置后只调用新的迁移协调器；调整网关存储错误映射和诊断日志，使调用链保留可观测原因而不泄露数据库内容。
5. 在 `app_runtime::run_app::setup` 建立启动门禁和原生错误呈现/退出路径，并重新排序所有现有异步初始化，确保门禁成功后才启动协议路由、网关、会话同步、scheduler、SSH 与其余后台服务。
6. 重写和补充 `shared_sqlite` 测试夹具，覆盖新库、旧库接管、严格拒绝路径、锁竞争、回滚与重复启动；执行专项测试后执行完整 Rust 检查。

## 任务边界与依赖

- 任务一：依赖与迁移资源。负责 Cargo 依赖锁定和四份嵌入 SQL 的规范路径，输出可供迁移协调器调用的编译资源；不修改启动流程。
- 任务二：共享 SQLite 迁移协调器。依赖任务一，负责 Refinery Runner、旧历史接管验证、错误分类与 `open_at` 集成；不调整 Tauri 服务调度。
- 任务三：应用启动门禁。依赖任务二公开的启动入口和错误分类，负责 `run_app` 的顺序、日志、原生对话框与退出；不改变网关业务 API。
- 任务四：迁移测试与回归验证。依赖前三项，负责替换旧迁移器专属断言、构造历史数据库夹具并运行完整验证。任务一完成后，任务二可独立进行；任务三和任务四必须在任务二完成后进行，任务四在任务三后收口应用级门禁验证。

## 具体改动

- `src-tauri/Cargo.toml` 与 `src-tauri/Cargo.lock`：加入 Refinery、升级 rusqlite 兼容版本并保留 bundled SQLite；不修改无关依赖。
- `src-tauri/migrations/V1__ai_routing_gateway.sql` 至 `V4__ai_routing_gateway.sql`：承载原有四个 schema 文件的未改动 SQL 内容，作为唯一的嵌入迁移来源；删除或停止引用旧 `src-tauri/src/ai_routing_gateway/schema_v*.sql` 路径，避免后续维护者误改失效副本。
- `src-tauri/src/shared_sqlite/migrations.rs`：删除自定义 `Migration`/`MIGRATIONS` 执行循环，新增嵌入迁移定义、历史表常量、旧历史读取与连续性检查、按版本的 schema 验证、已验证版本登记、Refinery 严格 Runner 调用及错误映射。
- `src-tauri/src/shared_sqlite/mod.rs`：保留路径、WAL、外键和 busy timeout 配置；将迁移调用接入新协调器，提供能表示锁超时、历史不可信、未来版本、校验和/资源缺失和执行失败的错误类型，并更新本模块测试。
- `src-tauri/src/ai_routing_gateway/storage.rs` 与 `src-tauri/src/ai_routing_gateway/commands/mod.rs`：映射或记录新的共享数据库失败类别，保持现有网关 runtime/scheduler 必须在 `prepare_startup` 成功后才启动的顺序。
- `src-tauri/src/app_runtime/run_app.rs`：在现有服务初始化和后台 spawn 前同步运行迁移门禁；接入已有 Tauri dialog 插件的原生消息对话框，记录详细错误后退出应用。

## 接口与数据流

- 启动数据流为：`run_app::setup` -> `shared_sqlite::open` -> 连接配置 -> 迁移协调器 -> 历史/结构验证或 Refinery 运行 -> 成功返回连接 -> 派生现有服务。任何失败不进入后续 spawn 路径。
- 后续网关和测试调用保留 `shared_sqlite::open/open_at -> rusqlite::Connection` 接口，因而现有存储消费者不需要迁移 API 改造；它们得到的是已经过迁移协调器验证的连接。
- `app_schema_migrations` 仅作为 legacy 读取输入：接管逻辑读取其 subsystem、version 和连续前缀，并将结果登记到 Refinery 专用历史表。成功后不再插入、更新或删除该旧表。
- Refinery 历史表成为唯一的写入型版本账本，保存版本、名称、执行时间和 checksum。版本目录统一使用全局正整数，Runner 根据其历史和嵌入资源决定待执行集。
- schema 验证器按 legacy 连续前缀验证应存在的表、列、索引、触发器和必要种子/约束；只比较该版本已发布契约所需对象，不把未知业务表作为可接管信号。

## 失败处理

- 迁移执行前用 `BEGIN IMMEDIATE` 或等价的 SQLite 写事务取得串行化锁；busy/locked 重试和总等待沿用 5 秒边界。超时映射为明确的锁等待失败，不能降级为普通可用连接。
- 旧历史表存在时，拒绝空洞前缀、重复或非正版本、超出 v4 的版本、未知 subsystem，或与对应版本 schema 契约不一致的数据库；拒绝前不得创建 Refinery 历史记录。
- Refinery 已有历史时，拒绝高于嵌入版本的记录、校验和分歧及历史中引用但二进制未嵌入的迁移。已发布 SQL 仅能由新版本迁移扩展，不能以改写文件修复。
- Runner 出错时保留 Refinery 的单版本事务回滚结果，关闭连接并返回分类错误；禁止启动网关 HTTP runtime、scheduler、协议路由及其他后台工作。
- 应用级门禁收到任何迁移阻塞错误时，记录结构化/可检索的错误类别和底层上下文，显示原生错误对话框并退出；对外对话框不包含敏感 SQL、密钥或业务数据。

## 测试与验证

- 在 `shared_sqlite` 单元测试中验证空数据库经嵌入 v1-v4 后的完整 schema、基础设置和默认组，并断言 Refinery 历史含四个版本、旧历史表没有新增写入。
- 为旧 v1、v2、v3、v4 连续前缀分别构造只含旧 `app_schema_migrations` 和相应已发布 schema 的夹具；接管后断言业务行保留、仅补齐缺失版本、Refinery 历史正确、重复打开不重复执行。
- 负向夹具覆盖缺号、未知版本、未知 subsystem、schema 缺表/列/索引/触发器、未来 Refinery 版本、校验和分歧和嵌入迁移缺失；每例断言失败发生于消费者访问前且 legacy 历史未被改写。
- 保留并适配单版本失败回滚测试，确认 SQL 对象和 Refinery 历史记录均未留下半完成状态；并发测试以多个连接竞争同一数据库，确认仅一个迁移执行路径及锁等待不超过 5 秒。
- 为 `run_app` 可抽取的启动门禁逻辑添加测试，断言迁移失败时不会调用服务启动/后台 spawn，成功时保持现有初始化顺序；原生对话框以可替换的报告边界测试，避免依赖图形环境。
- 在 `src-tauri` 目录执行 `cargo fmt --check`、迁移专项 `cargo test shared_sqlite`、完整 `cargo test` 和 `cargo check`；依赖升级后额外运行 `cargo tree` 审核 Refinery/rusqlite 解析结果。按 README 使用 `npm run tauri dev` 做人工启动烟测，确认空库与已接管库均可完成启动门禁。

## 兼容、迁移与发布

- 发布包继续把 SQLite 静态绑定在 Rust 二进制中，Refinery SQL 由编译宏内嵌；不要求用户安装迁移工具或部署 SQL 文件。
- 首次包含该改动的版本必须把 v1-v4 视为历史不可变资源。后续结构修复使用下一个全局版本文件，并保留历史文件和 checksum。
- 升级过程不提供自动备份或逆迁移。发布说明应明确：检测到无效旧历史、结构漂移、未来数据库或锁超时会阻止应用启动，需先恢复可信数据库或升级到匹配版本。
- 保持数据库路径、SQLite WAL 与外键配置，以及所有 `open/open_at` 调用方的连接接口不变，降低对现有网关存储和测试的兼容影响。
