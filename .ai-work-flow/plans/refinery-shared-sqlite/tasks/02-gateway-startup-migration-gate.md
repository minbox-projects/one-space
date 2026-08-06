# 02 - 接入启动迁移门控并完成全量回归验证

- task_id: `gateway-startup-migration-gate`
- order: `02`
- blocked_by: `refinery-dependency-runner`
- source_plan: `../plan.md`
- source_plan_digest: `50b29d6d6bcdd89a9825522ddc3f8375de5427bb3bfc5b3fe58734e81585ae7c`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src-tauri/src/app_runtime/run_app.rs`
  - `src-tauri/src/shared_sqlite/mod.rs`
  - `src-tauri/src/shared_sqlite/migrations.rs`
  - `src-tauri/src/ai_routing_gateway/commands/mod.rs`
  - `src-tauri/src/ai_routing_gateway/storage.rs`

## 预期结果

在 AI 路由网关初始化前显式等待共享数据库 bootstrap 成功，失败时阻断 initialize 和运行时暴露，并传播包含迁移阶段、数据库路径、识别版本、目标版本及非敏感底层原因的诊断。补齐全新库、v1-v4 原地升级、无历史完整 schema、已桥接库、非法状态、幂等、事务回滚、并发启动、启动顺序和连接配置回归矩阵；验证默认数据不重复、业务数据与密钥语义不受损、Refinery 历史连续且唯一。执行 scoped 测试、相关 crate 全量测试、cargo fmt --check、cargo check 和 rusqlite 依赖图检查，同时确认 app_store onboarding、OpenCode 读取路径保持不变，且未引入连接池或长期连接。

## 实施清单

- [x] 在 `run_app` 中建立显式共享数据库 bootstrap 边界，并确保该边界成功后才 spawn 或调用 AI 路由网关 `initialize`。
- [x] 将迁移失败区分为 `check`、`baseline`、`execute` 和 `commit` 阶段，安全传播数据库路径、识别旧版本、目标版本及底层 rusqlite/Refinery 原因，不包含密钥、token 或业务记录值。
- [x] 保证 bootstrap 失败时不创建、初始化或暴露网关运行时能力；网关初始化自身失败时与已成功提交的迁移结果分开报告。
- [x] 保持 `storage::open/open_at` 后续按操作调用共享 SQLite 的幂等行为，仅在接口契约确有需要时调整网关 commands 或 storage，不持有 bootstrap 连接。
- [x] 扩充回归矩阵，覆盖全新库、v1-v4、无历史完整 schema、已桥接库、非法状态、重复启动、事务失败、并发 bootstrap、启动顺序及连接配置。
- [x] 在并发 fixture 中验证多个线程或连接竞争同一临时数据库时，最终只有一份连续 Refinery 历史和完整 schema，BUSY/LOCKED 按既有超时及重试语义处理。
- [x] 验证默认 settings/default group 不重复，已有业务数据和密钥语义不改变，失败路径不留下部分 schema、基线或迁移历史。
- [x] 运行 scoped 测试和相关 crate 全量测试，并检查 app_store onboarding、文件迁移及 OpenCode 读取相关既有测试与调用路径未改变。
- [x] 执行格式、编译和依赖图检查，确认未引入连接池、长期连接或第二套 SQLite 链接实现。

## 验收标准

- [x] 启动成功路径严格表现为共享数据库迁移完成后才初始化网关；bootstrap 失败路径中 initialize 和网关运行时均未启动。
- [x] 错误诊断包含迁移阶段、数据库路径、识别版本、目标版本和非敏感底层原因，并能区分迁移失败与后续网关初始化失败。
- [x] 全新库、v1-v4、有无旧历史、已桥接库、非法状态、幂等、失败回滚和并发竞争测试全部通过，Refinery 历史始终连续且唯一。
- [x] 默认数据不重复，已有业务数据、配置和密钥语义保持不变，失败后无部分 schema、数据或历史提交。
- [x] 固定共享路径、三项 open flags、5000ms busy timeout、WAL、foreign keys、BUSY/LOCKED 重试及按操作开库行为无回归。
- [x] app_store onboarding 与文件迁移语义、外部 OpenCode 读取路径和生命周期保持不变，代码中未引入连接池或长期连接。

## 验证步骤

- [x] 运行 `cargo test --manifest-path src-tauri/Cargo.toml shared_sqlite`，预期完整迁移兼容、幂等、回滚、并发和连接配置矩阵通过。
- [x] 运行 `cargo test --manifest-path src-tauri/Cargo.toml app_runtime`，预期迁移成功先于网关初始化，迁移失败时 initialize 和运行时暴露均未发生。
- [x] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway`，预期网关生命周期、存储打开和错误映射相关测试通过。
- [x] 运行 `cargo test --manifest-path src-tauri/Cargo.toml`，预期相关 crate 全量测试通过，app_store 与 OpenCode 既有测试无回归。
- [x] 运行 `cargo fmt --manifest-path src-tauri/Cargo.toml --check` 和 `cargo check --manifest-path src-tauri/Cargo.toml`，预期格式与编译检查通过。
- [x] 运行 `cargo tree --manifest-path src-tauri/Cargo.toml -i rusqlite`，预期仍为兼容的单一 rusqlite/SQLite 依赖链。
- [x] 检查启动调用顺序和数据库连接持有范围，预期不存在迁移失败后初始化网关、连接池或 bootstrap 长期连接。

## 风险提示

- [x] 启动流程当前包含异步网关初始化；门控位置错误可能造成迁移与 initialize 竞态，测试必须直接断言调用顺序和失败阻断。
- [x] 并发测试需复用现有 5 秒 busy timeout 与 BUSY/LOCKED 重试，不能通过新增锁体系掩盖事务问题。
- [x] 诊断传播必须保留排障所需上下文，同时避免输出数据库中的密钥、token 或业务敏感值。

## 范围外事项

- `app_store` SQLite 化、onboarding 条件重构或文件迁移语义调整。
- 外部 OpenCode 数据库的管理、迁移、路径、schema 或连接生命周期调整。
- 连接池、长期连接、共享连接句柄或新的数据库所有权模型。
- AI 路由网关业务能力、业务 schema 或对外 API 的非必要改造。
